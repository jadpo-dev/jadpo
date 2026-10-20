//! Queues received as events (area 12, "Events from outside"; area 11,
//! "Queues"; chunk 7.7c).
//!
//! `application { receiving: { <connection>: { type_at: id_at: events: } } }`
//! names each queue the app reads: the connection to the package that
//! fetches it, where each message's type and id are in its body, and the
//! events it may carry; a queue of one kind lists one event and no
//! `type_at:`. Each listed event is the app's own (a domain's `events/`)
//! or a package's public one, and with `type_at:` each names the type it
//! is sent as with `wire_name:`, no two alike.
//!
//! The package says how its queue is read in `provides/receiving.jadpo`,
//! `provide receiving { receive: { } acknowledge: { } }`, mapping the
//! capability's two operations onto its own as data ([`mappings`]):
//! `receive:` calls an operation with its input mapped, and names where the
//! messages are in its answer (`response.<field>`), and each one's body and
//! the handle it is acknowledged by (`item.<field>`); `acknowledge:` calls
//! one with the handle (`message.handle`). An input's value is a setting
//! the package declares (`settings.<name>`), which each connection gives,
//! or a value written there.

use std::collections::BTreeMap;

use jadpo_db::{Db, Folder, Ownership, Place, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::Key;
use jadpo_syntax::ast::{self, AstNode, Decl, Expr};

use crate::connections::{Given, PackageConnection};
use crate::lookups::{Lookup, NameView};
use crate::resolve::{Names, Want};
use crate::std_lib::StdNames;
use crate::tree::{self, dotted, name_text, node_span};
use crate::types::{Const, FieldsBase, Prim, Ty, TypeRef, TypeView, type_table};

/// What one queue holds in `receiving:`.
pub const QUEUE_KEYS: [&str; 3] = ["type_at", "id_at", "events"];

/// The capability's operations a package maps, and what each takes.
pub const RECEIVE_KEYS: [&str; 5] = ["call", "input", "messages", "body", "handle"];
pub const ACKNOWLEDGE_KEYS: [&str; 2] = ["call", "input"];

/// A value a mapping gives an operation's input.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Mapped {
    /// `settings.<name>`: the package's setting, which each connection
    /// gives.
    Setting(String),
    Text(String),
    Int(i64),
    Bool(bool),
    /// `message.handle`.
    Handle,
}

/// One of the capability's operations, as a package maps it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MappedCall {
    /// `messages.receive_message`.
    pub operation: String,
    /// Each member of its input, by its spelling on the wire.
    pub input: Vec<(String, Mapped)>,
}

/// A package's `provide receiving`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Mapping {
    pub receive: MappedCall,
    /// Where the messages are in the receive's answer, and each one's body
    /// and handle, by field names.
    pub messages: Vec<String>,
    pub body: Vec<String>,
    pub handle: Vec<String>,
    pub acknowledge: MappedCall,
}

/// Every provider package's `provide receiving`, by its folder, and the
/// problems with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mappings {
    pub of: BTreeMap<String, Mapping>,
    /// The folders whose mapping doesn't read.
    pub broken: Vec<String>,
    pub problems: Vec<Diagnostic>,
}

/// An event a queue may carry.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct QueueEventModel {
    pub key: Key,
    pub name: String,
    pub wire_name: Option<String>,
}

/// A mapped input's value for one connection.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    /// Read from the app's setting of this name.
    Config(String),
    Text(String),
    Int(i64),
    Bool(bool),
    Handle,
}

/// One queue, read and checked.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct QueueModel {
    pub connection: String,
    pub type_at: Option<String>,
    pub id_at: String,
    pub events: Vec<QueueEventModel>,
    pub store: String,
    pub receive: (String, Vec<(String, Input)>),
    pub messages: Vec<String>,
    pub body: Vec<String>,
    pub handle: Vec<String>,
    pub acknowledge: (String, Vec<(String, Input)>),
}

/// Every queue the app receives, and the problems with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Receiving {
    pub queues: Vec<QueueModel>,
    pub problems: Vec<Diagnostic>,
}

/// The queues `receiving:` names, read.
#[salsa::tracked(returns(ref))]
pub fn receiving(db: &dyn Db, project: Project) -> Receiving {
    let mut out = Receiving::default();
    let mappings = mappings(db, project);
    let connections = &crate::connections::package_connections(db, project).connections;
    let lookup = Lookup::within(db, project, &Ownership::Root);
    for found in crate::config::singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("receiving") {
                continue;
            }
            let mut reader = Reader {
                db,
                project,
                path: &found.path,
                lookup: &lookup,
                out: &mut out.problems,
            };
            let Some(block) = member.block() else {
                reader.invalid(
                    node_span(&found.path, member.syntax()),
                    "`receiving:` isn't a block".to_string(),
                    "not a block",
                    "Write `receiving: { <connection>: { id_at: \"id\"  events: [<Event>] } }`",
                );
                continue;
            };
            for (name, entry) in tree::members(Some(block)) {
                let Some(connection) = name_text(&name) else {
                    continue;
                };
                let span = node_span(&found.path, name.syntax());
                if let Some(queue) = reader.queue(&connection, span, &entry, connections, mappings)
                {
                    out.queues.push(queue);
                }
            }
        }
    }
    out
}

/// The store the queues' events are kept in, when the app receives any:
/// what holds `jadpo_received_events` and `jadpo_parked_messages`.
pub fn store(db: &dyn Db, project: Project) -> Option<String> {
    receiving(db, project)
        .queues
        .first()
        .map(|queue| queue.store.clone())
}

struct Reader<'r, 'db> {
    db: &'db dyn Db,
    project: Project,
    path: &'r str,
    lookup: &'r Lookup<'db>,
    out: &'r mut Vec<Diagnostic>,
}

const REASON: &str = "A queue in `receiving:` is a connection to a package that provides `receiving`; it gives `id_at:`, where each message's id is in its body, `events:`, the events its messages may be, and `type_at:`, where each message's type is, unless it lists one event.";

impl Reader<'_, '_> {
    fn invalid(&mut self, span: Span, message: String, label: &str, next: &str) {
        self.out.push(
            Diagnostic::new(&rule::RECEIVING_INVALID, message)
                .at(span, label.to_string())
                .reason(REASON)
                .next(next.to_string()),
        );
    }

    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    fn queue(
        &mut self,
        name: &str,
        span: Span,
        entry: &ast::Member,
        connections: &[PackageConnection],
        mappings: &Mappings,
    ) -> Option<QueueModel> {
        let Some(connection) = connections.iter().find(|known| known.name == name) else {
            // A connection that doesn't read is reported where it is.
            if crate::connections::names_package(
                &crate::config::singletons(self.db, self.project, "application")
                    .first()
                    .and_then(|found| found.list.clone()),
                name,
            ) {
                return None;
            }
            self.invalid(
                span,
                format!("`{name}` isn't a connection to a package"),
                "not a queue's connection",
                &format!("Add `{name}` to `connections:` with the `package:` its queue is read through, or name one that is"),
            );
            return None;
        };
        let Some(mapping) = mappings.of.get(&connection.folder) else {
            if !mappings.broken.contains(&connection.folder) {
                self.invalid(
                    span,
                    format!("`{}` doesn't provide `receiving`", connection.package),
                    "can't fetch a queue",
                    "Name a connection whose package has `provides/receiving.jadpo`, such as `AmazonSqs.v2012_11_05`",
                );
            }
            return None;
        };
        let Some(block) = entry.block() else {
            self.invalid(
                self.span(entry),
                format!("`{name}` in `receiving:` isn't a block"),
                "not a block",
                "Write `<connection>: { id_at: \"id\"  events: [<Event>] }`",
            );
            return None;
        };
        let mut type_at = None;
        let mut id_at = None;
        let mut events = None;
        let mut listed = false;
        let mut ok = true;
        for (key, member) in tree::members(Some(block)) {
            let written = name_text(&key).unwrap_or_default();
            match written.as_str() {
                "type_at" | "id_at" => {
                    let place = match member.value().as_ref().map(Const::from_expr) {
                        Some(Const::Str(text)) if place_ok(&text) => Some(text),
                        _ => None,
                    };
                    match place {
                        Some(place) if written == "type_at" => type_at = Some(place),
                        Some(place) => id_at = Some(place),
                        None => {
                            ok = false;
                            self.invalid(
                                self.span(&member),
                                format!("`{written}:` isn't a place in a message"),
                                "not a place",
                                &format!("Write the member's name as text, dotted for one inside another, such as `{written}: \"type\"`"),
                            );
                        }
                    }
                }
                "events" => {
                    listed = true;
                    events = self.events(&member);
                }
                other => {
                    ok = false;
                    let near = nearest(other, QUEUE_KEYS.iter().copied());
                    self.invalid(
                        self.span(&key),
                        format!("A queue in `receiving:` has no key `{other}:`"),
                        "not a queue's key",
                        &match near {
                            Some(near) => format!("Use `{near}:` instead of `{other}:`"),
                            None => format!("Remove `{other}:`"),
                        },
                    );
                }
            }
        }
        let head = span.clone();
        let Some(id_at) = id_at else {
            if ok {
                self.invalid(
                    head,
                    format!("`{name}` doesn't say where each message's id is"),
                    "no `id_at:`",
                    "Add `id_at:` with the member holding each message's id, such as `id_at: \"message_id\"`; a repeat of an id is dropped",
                );
            }
            return None;
        };
        let Some(events) = events else {
            if ok && !listed {
                self.invalid(
                    head,
                    format!("`{name}` doesn't list the events its messages may be"),
                    "no `events:`",
                    "Add `events: [<Event>]`, each an event the app declares or a package's public one",
                );
            }
            return None;
        };
        if !ok {
            return None;
        }
        match &type_at {
            None if events.len() > 1 => {
                self.invalid(
                    head,
                    format!("`{name}` lists {} events but doesn't say where each message's type is", events.len()),
                    "no `type_at:`",
                    "Add `type_at:` with the member holding each message's type, such as `type_at: \"type\"`",
                );
                return None;
            }
            None => {}
            Some(_) => {
                let mut seen: BTreeMap<String, String> = BTreeMap::new();
                for (event, at) in &events {
                    let Some(wire_name) = &event.wire_name else {
                        self.invalid(
                            at.clone(),
                            format!("`{}` doesn't give `wire_name:`", event.name),
                            "no `wire_name:`",
                            "Add `wire_name:` to the event with the type its messages carry, such as `wire_name: \"order.placed\"`",
                        );
                        return None;
                    };
                    if let Some(first) = seen.insert(wire_name.clone(), event.name.clone()) {
                        self.invalid(
                            at.clone(),
                            format!("`{}` and `{first}` are both `{wire_name}`", event.name),
                            "named already",
                            &format!("Give `{}` a `wire_name:` of its own", event.name),
                        );
                        return None;
                    }
                }
            }
        }
        let Some(store) = crate::webhooks::store(self.db, self.project) else {
            let databases = crate::data::connections(self.db, self.project).databases();
            let store = databases.first().cloned().unwrap_or_else(|| "main".into());
            self.out.push(
                Diagnostic::new(
                    &rule::EVENT_STORE_REQUIRED,
                    format!("`{name}` receives a queue but has no store to keep its events in"),
                )
                .at(head, "no outbox")
                .reason("Each message's event is kept in the outbox, with its id so a repeat is dropped, until the app's subscribers have it: in the store `events: { store: }` names, or the only store that holds the app's records.")
                .next(format!(
                    "Add `events: {{ store: {store} }}` to `application {{ }}` in `app.jadpo`, naming a database that holds the app's records"
                )),
            );
            return None;
        };
        let inputs = |call: &MappedCall| -> Option<(String, Vec<(String, Input)>)> {
            let mut out = Vec::new();
            for (member, value) in &call.input {
                let given = match value {
                    Mapped::Setting(setting) => {
                        match connection
                            .settings
                            .iter()
                            .find(|(name, _)| name == setting)
                            .map(|(_, given)| given)?
                        {
                            Given::Setting(name) => Input::Config(name.clone()),
                            Given::Text(text) => Input::Text(text.clone()),
                            Given::Int(n) => Input::Int(*n),
                            Given::Bool(flag) => Input::Bool(*flag),
                            Given::Other => return None,
                        }
                    }
                    Mapped::Text(text) => Input::Text(text.clone()),
                    Mapped::Int(n) => Input::Int(*n),
                    Mapped::Bool(flag) => Input::Bool(*flag),
                    Mapped::Handle => Input::Handle,
                };
                out.push((member.clone(), given));
            }
            Some((call.operation.clone(), out))
        };
        Some(QueueModel {
            connection: name.to_string(),
            type_at,
            id_at,
            events: events.into_iter().map(|(event, _)| event).collect(),
            store,
            receive: inputs(&mapping.receive)?,
            messages: mapping.messages.clone(),
            body: mapping.body.clone(),
            handle: mapping.handle.clone(),
            acknowledge: inputs(&mapping.acknowledge)?,
        })
    }

    /// `events: [...]`: each an event, with where it is written.
    fn events(&mut self, member: &ast::Member) -> Option<Vec<(QueueEventModel, Span)>> {
        let Some(Expr::ListExpr(list)) = member.value() else {
            self.invalid(
                self.span(member),
                "`events:` isn't a list".to_string(),
                "not a list",
                "Write `events: [<Event>]`",
            );
            return None;
        };
        let mut out: Vec<(QueueEventModel, Span)> = Vec::new();
        let names = Names {
            path: self.path,
            ownership: &Ownership::Root,
            table: self.lookup,
            types: self.lookup,
            std: StdNames::get(),
            library: None,
        };
        let mut ok = true;
        for expr in list.exprs() {
            let at = self.span(&expr);
            // A name that doesn't resolve is reported where it is written.
            let mut problems = Vec::new();
            names.type_expr(&expr, Want::Type, &mut problems);
            if !problems.is_empty() {
                ok = false;
                continue;
            }
            let ty = self.lookup.resolve(&TypeRef::from_expr(&expr));
            let event = match &ty {
                Ty::Named(name) if self.lookup.base(name) == Some(FieldsBase::Event) => {
                    name.clone()
                }
                Ty::Error => {
                    ok = false;
                    continue;
                }
                _ => {
                    ok = false;
                    self.invalid(
                        at,
                        format!("`{}` isn't an event", tree::written(expr.syntax())),
                        "not an event",
                        "List events, each a type with the `Event` base",
                    );
                    continue;
                }
            };
            let Some(symbol) = self.lookup.declared(&event).first().cloned() else {
                ok = false;
                continue;
            };
            if matches!(symbol.ownership, Ownership::Package(_)) && !symbol.public {
                ok = false;
                self.invalid(
                    at,
                    format!(
                        "`{}` isn't one of its package's public events",
                        tree::written(expr.syntax())
                    ),
                    "not public",
                    "List a public event of the package, or the app's own",
                );
                continue;
            }
            let Some(key) = crate::graph::declaration_key(self.lookup, &event) else {
                ok = false;
                continue;
            };
            if out.iter().any(|(known, _)| known.key == key) {
                ok = false;
                self.invalid(
                    at,
                    format!("`events:` lists `{}` twice", tree::written(expr.syntax())),
                    "listed already",
                    "Remove the second",
                );
                continue;
            }
            let wire_name = match wire_name_of(self.db, self.project, &symbol) {
                Ok(wire_name) => wire_name,
                Err(span) => {
                    ok = false;
                    self.invalid(
                        span,
                        format!("`{event}`'s `wire_name:` isn't text"),
                        "not text",
                        "Write the type its messages carry as text, such as `wire_name: \"order.placed\"`",
                    );
                    continue;
                }
            };
            out.push((
                QueueEventModel {
                    key,
                    name: tree::written(expr.syntax()),
                    wire_name,
                },
                at,
            ));
        }
        if ok && out.is_empty() {
            self.invalid(
                self.span(member),
                "`events:` lists no event".to_string(),
                "empty",
                "List the events its messages may be",
            );
            return None;
        }
        ok.then_some(out)
    }
}

/// Whether `text` is a place in a body: dotted names, none empty.
fn place_ok(text: &str) -> bool {
    !text.is_empty() && text.split('.').all(|part| !part.trim().is_empty())
}

/// The `wire_name:` the declaration `symbol` gives itself: `None` when it
/// gives none, where it is written when it isn't text.
fn wire_name_of(
    db: &dyn Db,
    project: Project,
    symbol: &jadpo_db::Symbol,
) -> Result<Option<String>, Span> {
    let Some(file) = project
        .files(db)
        .iter()
        .copied()
        .find(|file| *file.path(db) == symbol.path)
    else {
        return Ok(None);
    };
    let Some(source) = tree::source(db, file) else {
        return Ok(None);
    };
    let Some(Decl::TypeDecl(decl)) = tree::decl_at(&source, symbol.index) else {
        return Ok(None);
    };
    for (key, member) in tree::members(decl.member_list()) {
        if name_text(&key).as_deref() != Some(crate::types::WIRE_NAME) {
            continue;
        }
        return match member.value().as_ref().map(Const::from_expr) {
            Some(Const::Str(text)) if !text.trim().is_empty() => Ok(Some(text)),
            _ => Err(node_span(&symbol.path, member.syntax())),
        };
    }
    Ok(None)
}

// --- the package's mapping ---------------------------------------------------

/// Every provider package's `provides/receiving.jadpo`, read and checked
/// against its operations and settings.
#[salsa::tracked(returns(ref))]
pub fn mappings(db: &dyn Db, project: Project) -> Mappings {
    let mut out = Mappings::default();
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        let (Ownership::Package(home), Place::Folder(Folder::Provides)) =
            (&location.ownership, &location.place)
        else {
            continue;
        };
        if location.stem != "receiving" || location.test {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let path = file.path(db).clone();
        let Some(decl) = source.decls().find_map(|decl| match decl {
            Decl::ProvideDecl(decl) => Some(decl),
            _ => None,
        }) else {
            continue;
        };
        let before = out.problems.len();
        let mut reader = MappingReader {
            db,
            project,
            path: &path,
            folder: &home.folder,
            settings: crate::connections::reach(db, project, home)
                .map(|reach| {
                    reach
                        .settings
                        .iter()
                        .map(|(name, _)| name.clone())
                        .collect()
                })
                .unwrap_or_default(),
            out: &mut out.problems,
        };
        let mapping = reader.mapping(&decl);
        match mapping {
            Some(mapping) if out.problems.len() == before => {
                out.of.insert(home.folder.clone(), mapping);
            }
            _ => out.broken.push(home.folder.clone()),
        }
    }
    out
}

struct MappingReader<'a> {
    db: &'a dyn Db,
    project: Project,
    path: &'a str,
    folder: &'a str,
    /// The settings its package declares.
    settings: Vec<String>,
    out: &'a mut Vec<Diagnostic>,
}

const MAPPING_REASON: &str = "`provide receiving` maps the capability's two operations onto the package's own, as data: `receive:` calls an operation with its `input:` and names where the `messages:` are in its answer (`response.<field>`) and each one's `body:` and `handle:` (`item.<field>`); `acknowledge:` calls one with the handle (`message.handle`). An input's value is `settings.<name>`, a setting the package declares, or a value written there.";

impl MappingReader<'_> {
    fn invalid(&mut self, span: Span, message: String, label: &str, next: String) {
        self.out.push(
            Diagnostic::new(&rule::PACKAGE_PROVIDER_INVALID, message)
                .at(span, label.to_string())
                .reason(MAPPING_REASON)
                .next(next),
        );
    }

    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    fn mapping(&mut self, decl: &ast::ProvideDecl) -> Option<Mapping> {
        let head = decl
            .name()
            .map_or_else(|| self.span(decl), |name| self.span(&name));
        let mut receive = None;
        let mut acknowledge = None;
        for (key, member) in tree::members(decl.member_list()) {
            let written = name_text(&key).unwrap_or_default();
            match written.as_str() {
                "receive" => receive = Some(member),
                "acknowledge" => acknowledge = Some(member),
                other => self.invalid(
                    self.span(&key),
                    format!("`provide receiving` has no operation `{other}:`"),
                    "not one of `receiving`'s operations",
                    format!("Remove `{other}:`; `receiving` has `receive:` and `acknowledge:`"),
                ),
            }
        }
        let (Some(receive), Some(acknowledge)) = (receive, acknowledge) else {
            self.invalid(
                head,
                "`provide receiving` doesn't map both `receive:` and `acknowledge:`".to_string(),
                "an operation not mapped",
                "Map `receive:` and `acknowledge:`, each onto one of the package's operations"
                    .to_string(),
            );
            return None;
        };
        let (call, members) = self.call(&receive, &RECEIVE_KEYS, false)?;
        let operation = crate::providers::operations(self.db, self.project)
            .of(self.folder)
            .find(|op| op.full_name() == call.operation)?
            .clone();
        let messages = members.get("messages");
        let body = members.get("body");
        let handle = members.get("handle");
        let (Some(messages), Some(body), Some(handle)) = (messages, body, handle) else {
            self.invalid(
                self.span(&receive),
                "`receive:` doesn't say where the messages, their bodies and their handles are"
                    .to_string(),
                "`messages:`, `body:` or `handle:` missing",
                "Add `messages: response.<field>`, `body: item.<field>` and `handle: item.<field>`"
                    .to_string(),
            );
            return None;
        };
        let table = type_table(self.db, self.project);
        let (messages_path, item) = self.list_at(messages, "response", &operation.output, table)?;
        let body_path = self.text_at(body, &item, table)?;
        let handle_path = self.text_at(handle, &item, table)?;
        let (acknowledged, _) = self.call(&acknowledge, &ACKNOWLEDGE_KEYS, true)?;
        Some(Mapping {
            receive: call,
            messages: messages_path,
            body: body_path,
            handle: handle_path,
            acknowledge: acknowledged,
        })
    }

    /// One operation's mapping: its call and input, and its other members
    /// by key.
    fn call(
        &mut self,
        member: &ast::Member,
        keys: &[&str],
        handle: bool,
    ) -> Option<(MappedCall, BTreeMap<String, ast::Member>)> {
        let Some(block) = member.block() else {
            self.invalid(
                self.span(member),
                "An operation's mapping isn't a block".to_string(),
                "not a block",
                "Write `{ call: <resource>.<operation>  input: { } }`".to_string(),
            );
            return None;
        };
        let mut members = BTreeMap::new();
        for (key, inner) in tree::members(Some(block)) {
            let written = name_text(&key).unwrap_or_default();
            if !keys.contains(&written.as_str()) {
                self.invalid(
                    self.span(&key),
                    format!("An operation's mapping has no key `{written}:`"),
                    "not a mapping key",
                    format!("Remove `{written}:`"),
                );
                return None;
            }
            members.insert(written, inner);
        }
        let Some(call) = members.get("call") else {
            self.invalid(
                self.span(member),
                "An operation's mapping doesn't give `call:`".to_string(),
                "no `call:`",
                "Add `call: <resource>.<operation>`, one of the package's operations".to_string(),
            );
            return None;
        };
        let written = call
            .value()
            .map(|value| tree::written(value.syntax()))
            .unwrap_or_default();
        let operations = crate::providers::operations(self.db, self.project);
        let found = written
            .split_once('.')
            .and_then(|(resource, name)| operations.find(self.folder, resource, name))
            .cloned();
        let Some(operation) = found else {
            let known: Vec<String> = operations
                .of(self.folder)
                .map(crate::providers::Operation::full_name)
                .collect();
            let next = match nearest(&written, known.iter().map(String::as_str)) {
                Some(near) => format!("Call `{near}`"),
                None => {
                    "Call one of the package's operations, `<resource>.<operation>`".to_string()
                }
            };
            self.invalid(
                call.value()
                    .map_or_else(|| self.span(call), |value| self.span(&value)),
                format!("`{written}` isn't one of the package's operations"),
                "no such operation",
                next,
            );
            return None;
        };
        if operation.decode.is_some() {
            self.invalid(
                self.span(call),
                format!("`{written}`'s answer is read by the package's own `decode:`"),
                "read by a function",
                "Map an operation whose output is read by its types".to_string(),
            );
            return None;
        }
        let table = type_table(self.db, self.project);
        let fields = match &operation.input {
            Some((Ty::Named(name), _)) => table.fields(name).map(<[_]>::to_vec).unwrap_or_default(),
            _ => Vec::new(),
        };
        let mut input = Vec::new();
        let given: Vec<(ast::Name, ast::Member)> = members
            .get("input")
            .map(|member| tree::members(member.block()))
            .unwrap_or_default();
        for (key, value) in &given {
            let name = name_text(key).unwrap_or_default();
            let Some(field) = fields.iter().find(|field| field.name == name) else {
                self.invalid(
                    self.span(key),
                    format!("`{written}`'s input has no field `{name}`"),
                    "not an input field",
                    format!("Map only the fields of `{written}`'s input"),
                );
                return None;
            };
            let mapped = self.value(value, handle)?;
            input.push((field.wire_name.clone().unwrap_or(name), mapped));
        }
        for field in &fields {
            let given = given
                .iter()
                .any(|(key, _)| name_text(key).as_deref() == Some(field.name.as_str()));
            if !given && !matches!(field.declared, Ty::Nullable(_)) && !field.optional {
                self.invalid(
                    self.span(member),
                    format!("`{written}`'s input field `{}` isn't mapped", field.name),
                    "a required field left out",
                    format!("Add `{}:` to `input:`", field.name),
                );
                return None;
            }
        }
        Some((
            MappedCall {
                operation: written,
                input,
            },
            members,
        ))
    }

    /// An input's value: `settings.<name>`, `message.handle` where the
    /// handle is given, or text, a whole number or a flag.
    fn value(&mut self, member: &ast::Member, handle: bool) -> Option<Mapped> {
        let value = member.value()?;
        if let Some((first, rest)) = dotted(&value) {
            let first = tree::name_ref_text(&first).unwrap_or_default();
            let rest: Vec<String> = rest.iter().filter_map(tree::name_ref_text).collect();
            match (first.as_str(), rest.as_slice()) {
                ("settings", [name]) if self.settings.contains(name) => {
                    return Some(Mapped::Setting(name.clone()));
                }
                ("message", [name]) if handle && name == "handle" => return Some(Mapped::Handle),
                _ => {
                    let next = if handle {
                        "Give `settings.<name>`, `message.handle`, or a value"
                    } else {
                        "Give `settings.<name>`, a setting the package declares, or a value"
                    };
                    self.invalid(
                        self.span(&value),
                        format!(
                            "`{}` isn't a value a mapping can give",
                            tree::written(value.syntax())
                        ),
                        "not mapped data",
                        next.to_string(),
                    );
                    return None;
                }
            }
        }
        match Const::from_expr(&value) {
            Const::Str(text) => Some(Mapped::Text(text)),
            Const::Bool(flag) => Some(Mapped::Bool(flag)),
            Const::Number {
                value: number,
                decimal: false,
            } if number.scale == 0 => i64::try_from(number.mantissa).ok().map(Mapped::Int),
            _ => {
                self.invalid(
                    self.span(&value),
                    format!(
                        "`{}` isn't a value a mapping can give",
                        tree::written(value.syntax())
                    ),
                    "not mapped data",
                    "Give `settings.<name>`, or text, a whole number or `true`".to_string(),
                );
                None
            }
        }
    }

    /// `response.<field>...`: a list of objects in the receive's answer,
    /// with the item's type.
    fn list_at(
        &mut self,
        member: &ast::Member,
        root: &str,
        output: &Ty,
        table: &dyn TypeView,
    ) -> Option<(Vec<String>, Ty)> {
        let (path, ty) = self.walk(member, root, output, table)?;
        let inner = match ty {
            Ty::Nullable(inner) => *inner,
            other => other,
        };
        match inner {
            Ty::List(item) => Some((path, *item)),
            _ => {
                self.invalid(
                    self.span(member),
                    "`messages:` isn't a list in the answer".to_string(),
                    "not a list",
                    "Name the answer's list of messages, `response.<field>`".to_string(),
                );
                None
            }
        }
    }

    /// `item.<field>...`: text in each message.
    fn text_at(
        &mut self,
        member: &ast::Member,
        item: &Ty,
        table: &dyn TypeView,
    ) -> Option<Vec<String>> {
        let (path, ty) = self.walk(member, "item", item, table)?;
        if table.base_prim(&ty) == Some(Prim::Text) {
            return Some(path);
        }
        self.invalid(
            self.span(member),
            format!(
                "`{}` isn't text",
                member
                    .value()
                    .map(|v| tree::written(v.syntax()))
                    .unwrap_or_default()
            ),
            "not text",
            "Name a text field of the message".to_string(),
        );
        None
    }

    /// The fields `<root>.<a>.<b>` names, from type `ty`, and the type at
    /// its end.
    fn walk(
        &mut self,
        member: &ast::Member,
        root: &str,
        ty: &Ty,
        table: &dyn TypeView,
    ) -> Option<(Vec<String>, Ty)> {
        let value = member.value()?;
        let parts = dotted(&value).map(|(first, rest)| {
            std::iter::once(first)
                .chain(rest)
                .filter_map(|part| tree::name_ref_text(&part))
                .collect::<Vec<_>>()
        });
        let Some(parts) = parts
            .filter(|parts| parts.first().map(String::as_str) == Some(root) && parts.len() > 1)
        else {
            self.invalid(
                self.span(&value),
                format!(
                    "`{}` isn't a place in `{root}`",
                    tree::written(value.syntax())
                ),
                "not a place",
                format!("Write `{root}.<field>`"),
            );
            return None;
        };
        let mut at = ty.clone();
        for name in &parts[1..] {
            let host = match &at {
                Ty::Named(host) => Some(host.clone()),
                Ty::Nullable(inner) => match inner.as_ref() {
                    Ty::Named(host) => Some(host.clone()),
                    _ => None,
                },
                _ => None,
            };
            let field = host.as_ref().and_then(|host| {
                table
                    .fields(host)
                    .and_then(|fields| fields.iter().find(|field| field.name == *name).cloned())
            });
            let Some(field) = field else {
                self.invalid(
                    self.span(&value),
                    format!("`{}` has no field `{name}`", tree::written(value.syntax())),
                    "no such field",
                    format!("Name a field of `{root}`"),
                );
                return None;
            };
            at = field.declared.clone();
        }
        Some((parts[1..].to_vec(), at))
    }
}
