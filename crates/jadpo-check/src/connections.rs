//! Connections to packages and the network report (area 11, "Connecting
//! in `app.jadpo`"; area 10, "Secrets"; chunk 7.1c).
//!
//! A connection in `application { connections: }` names a provider
//! package by its name and version (`package: AcmeCouriers.v1`), installed
//! in `packages/` or the app's own in `shared/providers/`, with no `use`
//! line, as the lock names it. It lists the domains whose actions may call
//! it (`domains:`), gives the credential the package's scheme sends from a
//! secret setting (`credential:`), chooses between the package's live and
//! sandbox `hosts:` with a `ProviderEnvironment` setting (`environment:`),
//! or, for a package with no fixed hosts, gives the address it is at
//! (`host:`, a `Url` setting), and gives each setting the package
//! declares in `provider.jadpo`'s `settings:` by name. Where an API is
//! comes from exactly one place, so `host:` beside fixed `hosts:` is
//! refused.
//!
//! [`package_connections`] reads and checks every such connection;
//! [`report`] renders the network report from them and the driver
//! connections: every host the app can reach, the connection that reaches
//! it, what it is reached for and the domains that use it.

use std::collections::BTreeSet;

use jadpo_db::{Db, PackageHome, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::json::Value;
use jadpo_syntax::ast::{self, AstNode, Decl, Expr};

use crate::config::{config, connection_urls, singletons};
use crate::data::ConnectionKind;
use crate::lookups::Lookup;
use crate::tree::{self, dotted, name_text, node_span};
use crate::types::{Const, Prim, TypeView};

/// The keys a connection to a package takes, besides the settings its
/// package declares (area 11, "Connecting in `app.jadpo`"). This chunk
/// reads the first five; the rest are read with the chunks that use them.
pub const KEYS: [&str; 13] = [
    "package",
    "domains",
    "credential",
    "environment",
    "host",
    "timeout",
    "retry",
    "max_concurrent",
    "webhooks",
    "erasure",
    "development",
    "polls",
    "outbound",
];

/// The credential scheme of AWS's Signature Version 4 (chunk 7.7c), whose
/// credential is a key id and a secret.
pub const AWS_SIGV4: &str = "aws_sigv4";

/// The types a provider's `settings:` may give a setting (chunk 7.1c):
/// the plain values a connection writes or reads from a setting.
pub const SETTING_TYPES: [Prim; 5] = [
    Prim::Text,
    Prim::Int,
    Prim::Decimal,
    Prim::Bool,
    Prim::Duration,
];

/// How a provider package's API is reached, from its `provider.jadpo`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Reach {
    /// `hosts: { live: sandbox: }`, as written, when it gives them.
    pub live: Option<String>,
    pub sandbox: Option<String>,
    /// Whether it gives `hosts:` at all.
    pub hosts: bool,
    /// `runs: { image: }`, when Jadpo runs it as a service.
    pub runs: Option<String>,
    /// Whether it gives `runs:` at all.
    pub runs_written: bool,
    /// Its credential scheme, `bearer`, when it sends one.
    pub credential: Option<String>,
    /// `credential_header:`, the header `CredentialScheme.header` sends the
    /// credential in (chunk 5.5v7).
    pub credential_header: Option<String>,
    /// `headers:`, the fixed headers every request carries, each its name
    /// and value (chunk 5.5v7).
    pub headers: Vec<(String, String)>,
    /// Each setting it declares, by name, with its type.
    pub settings: Vec<(String, Prim)>,
    /// `protocol:`'s variant, `json_http`.
    pub protocol: Option<String>,
    /// `idempotency:`, when its API takes a key (chunk 7.2c).
    pub idempotency: Option<Idempotency>,
    /// `error_code:`: where a failed answer's code is, dotted from its
    /// root (`error.code`).
    pub error_code: Option<String>,
    /// `webhooks:`, when the provider sends them (chunk 7.7).
    pub webhooks: Option<WebhookReach>,
    /// `aws: { service: target: }`, for an AWS API (chunk 7.7c): the
    /// service its requests are signed for and the prefix its operations
    /// are named under.
    pub aws: Option<AwsReach>,
    /// The names of its `polls:`, as written, when it gives the key (chunk
    /// 7.6c); [`crate::polls`] reads and checks each.
    pub polls: Option<Vec<String>>,
    /// `max_concurrent:`, how many of its calls go out at once per app
    /// instance, when it is a whole number of at least 1 (chunk 7.6c).
    pub max_concurrent: Option<u32>,
}

/// A provider's `aws:` (chunk 7.7c), as written; [`crate::packages`]
/// checks it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct AwsReach {
    pub service: Option<String>,
    pub target: Option<String>,
}

/// A value a connection gives one of its package's settings (chunk
/// 7.7c): a setting it reads, or one written in `app.jadpo`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Given {
    Setting(String),
    Text(String),
    Int(i64),
    Bool(bool),
    /// Another value, which nothing reads yet.
    Other,
}

/// A provider's `webhooks:` (area 11, "Receiving from outside"; chunk
/// 7.7): how its webhooks are signed and where each event's id, type and
/// time are in its body, as written; [`crate::packages`] checks them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct WebhookReach {
    /// `check:`'s scheme after `SignatureScheme.`, `stripe`.
    pub check: Option<String>,
    /// `header:`, the header `SignatureScheme.hmac_sha256` reads.
    pub header: Option<String>,
    /// `event_id:`, `event_type:` and `occurred_at:`, dotted from the
    /// body's root.
    pub event_id: Option<String>,
    pub event_type: Option<String>,
    pub occurred_at: Option<String>,
    /// `handshake:`'s rule after `WebhookHandshake.`, `sns` (chunk 7.7b).
    pub handshake: Option<String>,
}

/// A provider's `idempotency:` (area 11, "How the API is reached"): what
/// the runtime needs of it to send a call's derived key (chunk 7.2c).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Idempotency {
    /// The header the key goes in, `Idempotency-Key`.
    pub header: String,
    /// The longest key the provider takes.
    pub max_length: Option<u32>,
    /// The provider's error code for a key whose first request is still
    /// running.
    pub in_use: Option<String>,
    /// The provider's error code for a key sent with a different request.
    pub reused: Option<String>,
    /// How long the provider keeps a key, in milliseconds (`window:`).
    pub window_ms: Option<u64>,
    /// The provider's error code for a key that can't be used again.
    pub new_key_on: Option<String>,
    /// Whether the provider replays a failed answer under the same key.
    pub replays_failures: bool,
}

/// One connection to a provider package, as `app.jadpo` gives it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PackageConnection {
    pub name: String,
    /// Where its name is written: a path and its span.
    pub name_at: (String, u32, u32),
    /// The package's name and version, `AcmeCouriers.v1`.
    pub package: String,
    /// Its folder, `shared/providers/acme_couriers/v1`.
    pub folder: String,
    /// The domains whose actions may call it, in the order written.
    pub domains: Vec<String>,
    /// The settings its credential, environment and host are read from.
    pub credential: Option<String>,
    pub environment: Option<String>,
    pub host: Option<String>,
    /// `timeout:`, how long a call waits for its answer, in milliseconds,
    /// when the connection gives one (area 11, question 8; chunk 7.2c).
    pub timeout_ms: Option<u64>,
    /// `development: { sandbox: [...] }` (area 11, "Provider calls in
    /// development"; chunk 7.2d): the operations, `resource.operation`, and
    /// resources `jadpo dev` lets through to the provider's sandbox, each
    /// where it is written; checked against the package by
    /// [`connection_problems`].
    pub sandbox: Vec<(String, (String, u32, u32))>,
    /// `retry:`, how often and for how long a call that may be sent again
    /// is tried, when the connection gives it (area 11, question 8; chunk
    /// 7.2c2).
    pub retry: Option<jadpo_std::provider::RetryPlan>,
    /// `webhooks: { secret: config.<name> }`: the secret setting its
    /// provider's webhooks are checked with, when it receives them (area
    /// 11, "Receiving from outside"; chunk 7.7), and where it is written.
    pub webhook_secret: Option<WrittenSecret>,
    /// `webhooks: { replies: { } }` as written (chunk 7.7b): each event's
    /// lower_snake name and the action that answers it.
    pub webhook_replies: Vec<WrittenReply>,
    /// The setting an AWS credential's access key id is read from, beside
    /// its secret, `credential: { key_id: secret: }` (chunk 7.7c).
    pub key_id: Option<String>,
    /// Each setting of its package the connection gives, by name (chunk
    /// 7.7c).
    pub settings: Vec<(String, Given)>,
    /// `polls: { <name>: <interval> }` (chunk 7.6c): each of its package's
    /// polls called at another interval, in milliseconds.
    pub polls: Vec<(String, u64)>,
    /// `max_concurrent:`, lowering its package's (chunk 7.6c).
    pub max_concurrent: Option<u32>,
    pub reach: Reach,
}

/// A secret setting's name and where it is written: a path and its span.
pub type WrittenSecret = (String, (String, u32, u32));

/// One entry of a connection's `webhooks: { replies: }` (area 11,
/// "Webhooks that need a reply"; chunk 7.7b): `incoming_call:
/// support.answer_call`, read as written; [`crate::webhooks`] finds the
/// event and the action.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WrittenReply {
    /// The event's name in lower_snake_case, `incoming_call`.
    pub event: String,
    /// Where its key and its value are written: a path and its span.
    pub key_at: (String, u32, u32),
    /// The action's name as written, `support.answer_call`, or `None`
    /// when it isn't a name.
    pub action: Option<Vec<String>>,
    pub value_at: (String, u32, u32),
}

impl WrittenReply {
    pub fn key_span(&self) -> Span {
        Span::new(self.key_at.0.clone(), self.key_at.1, self.key_at.2)
    }

    pub fn value_span(&self) -> Span {
        Span::new(self.value_at.0.clone(), self.value_at.1, self.value_at.2)
    }
}

/// The connections to packages, and the problems with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageConnections {
    pub connections: Vec<PackageConnection>,
    pub problems: Vec<Diagnostic>,
}

/// Every connection to a package, read and checked.
#[salsa::tracked(returns(ref))]
pub fn package_connections(db: &dyn Db, project: Project) -> PackageConnections {
    let mut out = PackageConnections::default();
    let homes = crate::surface::homes(db, project);
    let table = jadpo_db::symbols(db, project);
    let domains: Vec<String> = table.domains.iter().cloned().collect();
    let emailed = crate::emails::written_connection(db, project);
    for found in singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("connections") {
                continue;
            }
            for (name, entry) in tree::members(member.block()) {
                let Some(connection) = name_text(&name) else {
                    continue;
                };
                let settings = tree::members(entry.block());
                let Some((_, package)) = settings
                    .iter()
                    .find(|(key, _)| name_text(key).as_deref() == Some("package"))
                else {
                    continue;
                };
                let reader = Reader {
                    db,
                    project,
                    path: &found.path,
                    connection: &connection,
                    name_span: node_span(&found.path, name.syntax()),
                    domains: &domains,
                    emailed: emailed.as_deref() == Some(connection.as_str()),
                };
                let Some(home) = reader.package(package, &homes, &mut out.problems) else {
                    continue;
                };
                let Some(reach) = reach(db, project, &home) else {
                    out.problems.push(reader.library(package, &home));
                    continue;
                };
                if let Some(read) = reader.read(&home, &reach, &settings, &mut out.problems) {
                    out.connections.push(read);
                }
            }
        }
    }
    out
}

/// Whether `application { }`'s block `list` declares connection `name`
/// with a `package:`, read or not.
pub fn names_package(list: &Option<ast::MemberList>, name: &str) -> bool {
    tree::members(list.clone())
        .into_iter()
        .filter(|(key, _)| name_text(key).as_deref() == Some("connections"))
        .flat_map(|(_, member)| tree::members(member.block()))
        .filter(|(key, _)| name_text(key).as_deref() == Some(name))
        .any(|(_, entry)| {
            tree::members(entry.block())
                .iter()
                .any(|(key, _)| name_text(key).as_deref() == Some("package"))
        })
}

/// The problems with the connections to packages and databases, and
/// every connection whose name is a domain's or a library's.
#[salsa::tracked(returns(ref))]
pub fn connection_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = package_connections(db, project).problems.clone();
    out.extend(
        crate::databases::database_connections(db, project)
            .problems
            .iter()
            .cloned(),
    );
    out.extend(name_clashes(db, project));
    out.extend(sandbox_problems(db, project));
    out
}

/// The operations a connection's `development: { sandbox: }` lets
/// through (chunk 7.2d): each named operation, and every public operation
/// of each named resource, by `resource.operation`.
pub fn sandboxed(db: &dyn Db, project: Project, connection: &PackageConnection) -> Vec<String> {
    let operations = crate::providers::operations(db, project);
    let mut out: Vec<String> = Vec::new();
    let prompts = asked_prompts(db, project, connection);
    for (name, _) in &connection.sandbox {
        if prompts.contains(name) && !out.contains(name) {
            out.push(name.clone());
        }
        for operation in operations.of(&connection.folder) {
            let full = operation.full_name();
            if operation.public
                && (full == *name || operation.resource == *name)
                && !out.contains(&full)
            {
                out.push(full);
            }
        }
    }
    out
}

/// The prompts a connection's `sandbox:` may name, by their names: the
/// application's own, when the connection is the one `ai:` names (area 21,
/// "Tests"; chunk 5.5v3b), each let through to the provider's sandbox in
/// `jadpo dev` with the developer's key.
fn asked_prompts(db: &dyn Db, project: Project, connection: &PackageConnection) -> Vec<String> {
    if crate::ai::settings(db, project).named.as_deref() != Some(connection.name.as_str()) {
        return Vec::new();
    }
    let mut out: Vec<String> = crate::prompts::analysis(db, project)
        .prompts
        .iter()
        .filter(|prompt| prompt.ownership.package().is_none())
        .map(|prompt| prompt.key.clone())
        .collect();
    // Its embeddings, by the operation they are logged as, where its
    // package maps `embed:` (chunk 5.5v7).
    if crate::ai_mapping::maps(db, project, &connection.name, "embed") == Some(true) {
        out.push(jadpo_std::embed::OPERATION.to_string());
    }
    out
}

/// Each name a connection's `sandbox:` lists that is no public operation
/// or resource of its package, nor a prompt of the application's when it
/// fills `ai`.
fn sandbox_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let operations = crate::providers::operations(db, project);
    let mut out = Vec::new();
    for connection in &package_connections(db, project).connections {
        if connection.sandbox.is_empty()
            || crate::providers::has_problems(db, project, &connection.folder)
        {
            continue;
        }
        let public: Vec<&crate::providers::Operation> = operations
            .of(&connection.folder)
            .filter(|operation| operation.public)
            .collect();
        let mut known: Vec<String> = public
            .iter()
            .map(|operation| operation.full_name())
            .collect();
        for operation in &public {
            if !known.contains(&operation.resource) {
                known.push(operation.resource.clone());
            }
        }
        for prompt in asked_prompts(db, project, connection) {
            if !known.contains(&prompt) {
                known.push(prompt);
            }
        }
        for (name, (path, start, end)) in &connection.sandbox {
            if known.contains(name) {
                continue;
            }
            let span = Span::new(path.clone(), *start, *end);
            let near = nearest(name, known.iter().map(String::as_str));
            let mut problem = Diagnostic::new(
                &rule::CONNECTION_VALUE_INVALID,
                format!(
                    "`{}`'s `sandbox:` lists `{name}`, which `{}` doesn't have",
                    connection.name, connection.package
                ),
            )
            .at(span.clone(), "no such operation or resource")
            .reason(format!(
                "`sandbox:` lets the package's public operations and resources through to its sandbox in `jadpo dev`; `{}` has {}.",
                connection.package,
                if known.is_empty() {
                    "none".to_string()
                } else {
                    crate::and_list(&known.iter().map(|name| format!("`{name}`")).collect::<Vec<_>>())
                }
            ));
            problem = match near {
                Some(near) => problem
                    .next(format!("Use `{near}` instead of `{name}`"))
                    .edit(span.clone(), near),
                None => problem.next(format!("Remove `{name}`")),
            };
            out.push(problem);
        }
    }
    out
}

/// A connection's name shares one namespace with the app's domains and
/// libraries (area 11, question 17), since `name.call(...)` has to mean
/// one thing: a connection named like either is reported at the
/// connection, the one `app.jadpo` can rename without moving code.
fn name_clashes(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let table = jadpo_db::symbols(db, project);
    let mut out = Vec::new();
    for found in singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("connections") {
                continue;
            }
            for (name, _) in tree::members(member.block()) {
                let Some(connection) = name_text(&name) else {
                    continue;
                };
                let library = table.declarations.get(&connection).and_then(|found| {
                    found.iter().find(|symbol| {
                        symbol.kind == jadpo_db::ItemKind::Library
                            && symbol.library.is_none()
                            && symbol.ownership.package().is_none()
                    })
                });
                let (what, label) = if table.domains.contains(&connection) {
                    (format!("domain `{connection}`"), "already a domain")
                } else if let Some(library) = library {
                    (
                        format!("library `{connection}` in `{}`", library.path),
                        "already a library",
                    )
                } else {
                    continue;
                };
                out.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_DUPLICATE_DECLARATION,
                        format!("Connection `{connection}` is named like {what}"),
                    )
                    .at(node_span(&found.path, name.syntax()), label)
                    .reason(format!(
                        "Connections share one namespace with domains and libraries, so `{connection}.<name>` means one thing; it is already the {what}."
                    ))
                    .next(format!(
                        "Rename the connection, such as `{connection}_api`, and the calls through it"
                    )),
                );
            }
        }
    }
    out
}

/// `provider.jadpo`'s word for how the package in `home` is reached;
/// `None` for a library package, which has none.
pub fn reach(db: &dyn Db, project: Project, home: &PackageHome) -> Option<Reach> {
    let path = format!("{}/{}", home.folder, jadpo_db::PROVIDER_FILE);
    let file = project
        .files(db)
        .iter()
        .copied()
        .find(|file| *file.path(db) == path)?;
    let mut reach = Reach::default();
    let Some(source) = tree::source(db, file) else {
        return Some(reach);
    };
    let Some(decl) = source.decls().find_map(|decl| match decl {
        Decl::SingletonDecl(decl) if decl.keyword().is_some_and(|k| k.text() == "provider") => {
            Some(decl)
        }
        _ => None,
    }) else {
        return Some(reach);
    };
    for (key, member) in tree::members(decl.member_list()) {
        match name_text(&key).as_deref() {
            Some("hosts") => {
                reach.hosts = true;
                for (host, inner) in tree::members(member.block()) {
                    let value = text_of(&inner);
                    match name_text(&host).as_deref() {
                        Some("live") => reach.live = value,
                        Some("sandbox") => reach.sandbox = value,
                        _ => {}
                    }
                }
            }
            Some("runs") => {
                reach.runs_written = true;
                reach.runs = tree::members(member.block())
                    .iter()
                    .find(|(key, _)| name_text(key).as_deref() == Some("image"))
                    .and_then(|(_, inner)| text_of(inner));
            }
            Some("protocol") => {
                reach.protocol = member.value().map(|value| {
                    let written = tree::written(value.syntax());
                    written
                        .strip_prefix("Protocol.")
                        .unwrap_or(&written)
                        .to_string()
                });
            }
            Some("error_code") => reach.error_code = text_of(&member),
            Some("polls") => {
                reach.polls = Some(
                    tree::members(member.block())
                        .iter()
                        .filter_map(|(name, _)| name_text(name))
                        .collect(),
                );
            }
            Some("max_concurrent") => reach.max_concurrent = whole(&member),
            Some("aws") => {
                let mut aws = AwsReach::default();
                for (key, inner) in tree::members(member.block()) {
                    match name_text(&key).as_deref() {
                        Some("service") => aws.service = text_of(&inner),
                        Some("target") => aws.target = text_of(&inner),
                        _ => {}
                    }
                }
                reach.aws = Some(aws);
            }
            Some("webhooks") => {
                let mut webhooks = WebhookReach::default();
                for (key, inner) in tree::members(member.block()) {
                    match name_text(&key).as_deref() {
                        Some("check") => {
                            webhooks.check = inner.value().map(|value| {
                                let written = tree::written(value.syntax());
                                written
                                    .strip_prefix("SignatureScheme.")
                                    .unwrap_or(&written)
                                    .to_string()
                            });
                        }
                        Some("header") => webhooks.header = text_of(&inner),
                        Some("event_id") => webhooks.event_id = text_of(&inner),
                        Some("event_type") => webhooks.event_type = text_of(&inner),
                        Some("occurred_at") => webhooks.occurred_at = text_of(&inner),
                        Some("handshake") => {
                            webhooks.handshake = inner.value().map(|value| {
                                let written = tree::written(value.syntax());
                                written
                                    .strip_prefix("WebhookHandshake.")
                                    .unwrap_or(&written)
                                    .to_string()
                            });
                        }
                        _ => {}
                    }
                }
                reach.webhooks = Some(webhooks);
            }
            Some("idempotency") => {
                let mut idempotency = Idempotency::default();
                for (key, inner) in tree::members(member.block()) {
                    match name_text(&key).as_deref() {
                        Some("header") => idempotency.header = text_of(&inner).unwrap_or_default(),
                        Some("max_length") => {
                            idempotency.max_length =
                                match inner.value().as_ref().map(Const::from_expr) {
                                    Some(Const::Number {
                                        value,
                                        decimal: false,
                                    }) => (value.scale == 0)
                                        .then_some(value.mantissa)
                                        .and_then(|n| u32::try_from(n).ok()),
                                    _ => None,
                                };
                        }
                        Some("in_use") => idempotency.in_use = text_of(&inner),
                        Some("reused") => idempotency.reused = text_of(&inner),
                        Some("new_key_on") => idempotency.new_key_on = text_of(&inner),
                        Some("window") => {
                            idempotency.window_ms =
                                match inner.value().as_ref().map(Const::from_expr) {
                                    Some(Const::Duration(text)) => jadpo_std::Dur::parse(&text)
                                        .and_then(|dur| u64::try_from(dur.millis).ok()),
                                    _ => None,
                                };
                        }
                        Some("replays_failures") => {
                            idempotency.replays_failures = inner
                                .value()
                                .is_some_and(|value| tree::written(value.syntax()) == "true");
                        }
                        _ => {}
                    }
                }
                reach.idempotency = Some(idempotency);
            }
            Some("credential") => {
                reach.credential = member.value().map(|value| {
                    let written = tree::written(value.syntax());
                    written
                        .strip_prefix("CredentialScheme.")
                        .unwrap_or(&written)
                        .to_string()
                });
            }
            Some("credential_header") => reach.credential_header = text_of(&member),
            Some("headers") => {
                if let Some(Const::List(items)) = member.value().as_ref().map(Const::from_expr) {
                    reach.headers = items
                        .into_iter()
                        .filter_map(|item| match item {
                            Const::Str(line) => crate::packages::header_line(&line),
                            _ => None,
                        })
                        .collect();
                }
            }
            Some("settings") => {
                for (setting, inner) in tree::members(member.block()) {
                    let (Some(setting), Some(value)) = (name_text(&setting), inner.value()) else {
                        continue;
                    };
                    if let Some(prim) = Prim::from_name(&tree::written(value.syntax()))
                        && SETTING_TYPES.contains(&prim)
                    {
                        reach.settings.push((setting, prim));
                    }
                }
            }
            _ => {}
        }
    }
    Some(reach)
}

/// A member's whole number of at least 1, as `max_concurrent:` takes.
pub(crate) fn whole(member: &ast::Member) -> Option<u32> {
    match member.value().as_ref().map(Const::from_expr) {
        Some(Const::Number {
            value,
            decimal: false,
        }) if value.scale == 0 => u32::try_from(value.mantissa).ok().filter(|n| *n >= 1),
        _ => None,
    }
}

/// A string member's text, its escapes decoded.
fn text_of(member: &ast::Member) -> Option<String> {
    match member.value().as_ref().map(Const::from_expr) {
        Some(Const::Str(text)) => Some(text),
        _ => None,
    }
}

/// `settings:` in `provider.jadpo`: each a setting's name and a type a
/// connection can give (`package.provider_invalid` otherwise).
pub(crate) fn provider_settings(path: &str, member: &ast::Member, out: &mut Vec<Diagnostic>) {
    let names: Vec<String> = SETTING_TYPES
        .iter()
        .map(|prim| format!("`{}`", prim.name()))
        .collect();
    for (setting, inner) in tree::members(member.block()) {
        let written = inner
            .value()
            .map(|value| tree::written(value.syntax()))
            .unwrap_or_default();
        let fits = Prim::from_name(&written).is_some_and(|prim| SETTING_TYPES.contains(&prim));
        if fits {
            continue;
        }
        let setting = name_text(&setting).unwrap_or_default();
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_PROVIDER_INVALID,
                format!("Setting `{setting}:` can't be a `{written}`"),
            )
            .at(
                inner.value().map_or_else(
                    || node_span(path, inner.syntax()),
                    |value| node_span(path, value.syntax()),
                ),
                "not a setting's type",
            )
            .reason(format!(
                "A package's `settings:` are plain values each connection gives by name, written in `app.jadpo` or read from a setting: {}.",
                crate::or_list(&names)
            ))
            .next(format!("Give `{setting}:` one of those types, such as `Text`")),
        );
    }
}

/// Reads one connection's keys.
struct Reader<'a> {
    db: &'a dyn Db,
    project: Project,
    path: &'a str,
    connection: &'a str,
    name_span: Span,
    domains: &'a [String],
    /// Whether `email:` names this connection, which then needs no
    /// `domains:` (area 16, "The connection"; chunk 7.8d).
    emailed: bool,
}

impl Reader<'_> {
    /// The package `package:` names, among those the project holds.
    fn package(
        &self,
        member: &ast::Member,
        homes: &std::collections::BTreeMap<String, PackageHome>,
        out: &mut Vec<Diagnostic>,
    ) -> Option<PackageHome> {
        let value = member.value()?;
        let span = node_span(self.path, value.syntax());
        let written = tree::written(value.syntax());
        if let Some(home) = homes.values().find(|home| home.versioned() == written) {
            return Some(home.clone());
        }
        let code = written.split('.').next().unwrap_or_default();
        let held: Vec<&PackageHome> = homes.values().filter(|home| home.code == code).collect();
        let mut problem = Diagnostic::new(
            &rule::CONNECTION_PACKAGE_UNKNOWN,
            format!(
                "`{}` connects to `{written}`, which the app doesn't hold",
                self.connection
            ),
        )
        .at(span.clone(), "no such package");
        problem = match held.as_slice() {
            [one] => problem
                .reason(format!(
                    "A connection's `package:` names a provider package by its name and version, installed in `packages/` or the app's own in `shared/providers/`; the app holds `{}`.",
                    one.versioned()
                ))
                .next(format!("Write `package: {}`", one.versioned()))
                .edit(span, one.versioned()),
            [] => problem
                .reason(format!(
                    "A connection's `package:` names a provider package by its name and version, installed in `packages/` or the app's own in `shared/providers/`; the app holds no package called `{code}`."
                ))
                .next("Install it with `jadpo package add`, or write it as the app's own integration in `shared/providers/<name>/<version>/`"),
            many => problem
                .reason(format!(
                    "A connection's `package:` names a provider package by its name and version, installed in `packages/` or the app's own in `shared/providers/`; the app holds {}.",
                    crate::and_list(
                        &many
                            .iter()
                            .map(|home| format!("`{}`", home.versioned()))
                            .collect::<Vec<_>>()
                    )
                ))
                .next("Write the version the app holds"),
        };
        out.push(problem);
        None
    }

    /// A connection to a library package, which needs none.
    fn library(&self, member: &ast::Member, home: &PackageHome) -> Diagnostic {
        let span = member.value().map_or_else(
            || node_span(self.path, member.syntax()),
            |value| node_span(self.path, value.syntax()),
        );
        Diagnostic::new(
            &rule::CONNECTION_PACKAGE_UNKNOWN,
            format!(
                "`{}` connects to library package `{}`",
                self.connection,
                home.versioned()
            ),
        )
        .at(span, "a library package")
        .reason(format!(
            "A library package carries types, failures and functions and calls no API, so it needs no connection: any domain brings it in with `use {}`.",
            home.use_path()
        ))
        .next(format!(
            "Remove the `{}` connection, and add `use {}` where its names are used",
            self.connection,
            home.use_path()
        ))
    }

    /// The connection's other keys, against what its package takes.
    fn read(
        &self,
        home: &PackageHome,
        reach: &Reach,
        settings: &[(ast::Name, ast::Member)],
        out: &mut Vec<Diagnostic>,
    ) -> Option<PackageConnection> {
        let package = home.versioned();
        let before = out.len();
        let mut read = PackageConnection {
            name: self.connection.to_string(),
            name_at: (
                self.name_span.path.clone(),
                self.name_span.start,
                self.name_span.end,
            ),
            package: package.clone(),
            folder: home.folder.clone(),
            domains: Vec::new(),
            credential: None,
            environment: None,
            host: None,
            timeout_ms: None,
            sandbox: Vec::new(),
            retry: None,
            webhook_secret: None,
            webhook_replies: Vec::new(),
            key_id: None,
            settings: Vec::new(),
            polls: Vec::new(),
            max_concurrent: None,
            reach: reach.clone(),
        };
        let mut given: BTreeSet<String> = BTreeSet::new();
        for (key, member) in settings {
            let Some(written) = name_text(key) else {
                continue;
            };
            given.insert(written.clone());
            let key_span = node_span(self.path, key.syntax());
            match written.as_str() {
                "package" => {}
                "domains" => read.domains = self.domains(member, out),
                "timeout" => read.timeout_ms = self.timeout(member, out),
                "development" => read.sandbox = self.development(member, out),
                "retry" => read.retry = self.retry(member, out),
                "max_concurrent" => read.max_concurrent = self.max_concurrent(member, reach, out),
                "polls" => match &reach.polls {
                    Some(names) => read.polls = self.polls(member, names, out),
                    None => out.push(
                        self.not_taken(key_span, "polls", &package)
                            .reason(format!(
                                "`polls:` changes how often the package's polls run, and `{package}`'s `provider.jadpo` declares none."
                            ))
                            .next("Remove `polls:`"),
                    ),
                },
                "webhooks" => match &reach.webhooks {
                    Some(_) => {
                        (read.webhook_secret, read.webhook_replies) = self.webhooks(member, out);
                    }
                    None => out.push(
                        self.not_taken(key_span, "webhooks", &package)
                            .reason(format!(
                                "`{package}`'s `provider.jadpo` declares no `webhooks:`, so its provider sends none to receive."
                            ))
                            .next("Remove `webhooks:`, or declare how the provider signs its webhooks in the package's `provider.jadpo`"),
                    ),
                },
                "credential" => match &reach.credential {
                    Some(scheme) if scheme == AWS_SIGV4 => {
                        if let Some((key_id, secret)) = self.aws_credential(member, out) {
                            read.key_id = Some(key_id);
                            read.credential = Some(secret);
                        }
                    }
                    Some(_) => read.credential = self.credential(member, out),
                    None => out.push(
                        self.not_taken(key_span, "credential", &package)
                            .reason(format!(
                                "`{package}`'s `provider.jadpo` names no credential scheme, so its API is called without one."
                            ))
                            .next("Remove `credential:`, or give the package's `provider.jadpo` the `credential:` scheme its API needs"),
                    ),
                },
                "environment" => {
                    if reach.hosts {
                        read.environment = self.environment(member, out);
                    } else {
                        out.push(
                            self.not_taken(key_span, "environment", &package)
                                .reason(format!(
                                    "`environment:` chooses between a package's live and sandbox `hosts:`, and `{package}` has none: {}",
                                    self.elsewhere(reach)
                                ))
                                .next("Remove `environment:`"),
                        );
                    }
                }
                "host" => {
                    if reach.hosts {
                        out.push(
                            Diagnostic::new(
                                &rule::CONNECTION_KEY_INVALID,
                                format!(
                                    "`{}` gives `host:`, but `{package}` has fixed hosts",
                                    self.connection
                                ),
                            )
                            .at(key_span, "a second place for its address")
                            .reason(format!(
                                "Where an API is comes from exactly one place: `{package}`'s `hosts:` give its live and sandbox addresses, and `environment:` chooses between them."
                            ))
                            .next("Remove `host:`"),
                        );
                    } else {
                        read.host = self.host(member, out);
                    }
                }
                other if KEYS.contains(&other) => {}
                other => {
                    if let Some((_, prim)) =
                        reach.settings.iter().find(|(setting, _)| setting == other)
                    {
                        self.setting(other, *prim, &package, member, out);
                        read.settings.push((other.to_string(), self.given(member)));
                    } else {
                        out.push(self.unknown(key_span, other, &package, reach));
                    }
                }
            }
        }
        let missing = |key: &str, reason: String, next: String| {
            Diagnostic::new(
                &rule::CONNECTION_KEY_MISSING,
                format!("`{}` doesn't give `{key}:`", self.connection),
            )
            .at(self.name_span.clone(), format!("no `{key}:`"))
            .reason(reason)
            .next(next)
        };
        if !given.contains("domains") && !self.emailed {
            out.push(missing(
                "domains",
                "A connection lists the domains whose actions may call it, so a new caller is a one-line change a reviewer sees; other domains go through those domains' public actions.".to_string(),
                format!("Add `domains: [<domain>]`, listing the domains that call `{}`", self.connection),
            ));
        }
        if let Some(scheme) = &reach.credential
            && !given.contains("credential")
        {
            out.push(missing(
                "credential",
                format!(
                    "`{package}` sends a `{scheme}` credential with every call, which the connection gives from a secret setting."
                ),
                if scheme == AWS_SIGV4 {
                    "Add `credential: { key_id: config.<name>  secret: config.<name> }`, the secret a `secret: true` setting".to_string()
                } else {
                    "Declare `<name>: Text { secret: true }` in `config.jadpo`, and add `credential: config.<name>`".to_string()
                },
            ));
        }
        if reach.hosts && !given.contains("environment") {
            out.push(missing(
                "environment",
                format!(
                    "`{package}` has a live and a sandbox host, and `environment:`, a `ProviderEnvironment` setting, chooses between them, so each deployment says which it calls."
                ),
                "Declare `<name>: ProviderEnvironment` in `config.jadpo`, and add `environment: config.<name>`".to_string(),
            ));
        }
        if !reach.hosts && !reach.runs_written && !given.contains("host") {
            out.push(missing(
                "host",
                format!(
                    "`{package}` has no fixed `hosts:` and Jadpo doesn't run it, so the connection says where it is."
                ),
                "Declare `<name>: Url` in `config.jadpo`, and add `host: config.<name>`".to_string(),
            ));
        }
        for (setting, prim) in &reach.settings {
            if !given.contains(setting) {
                out.push(missing(
                    setting,
                    format!(
                        "`{package}` declares the setting `{setting}:`, a `{}`, which every connection to it gives.",
                        prim.name()
                    ),
                    format!("Add `{setting}:` with a `{}` value", prim.name()),
                ));
            }
        }
        (out.len() == before).then_some(read)
    }

    /// Where a package without `hosts:` is, for a reason.
    fn elsewhere(&self, reach: &Reach) -> &'static str {
        if reach.runs_written {
            "Jadpo runs it as a service."
        } else {
            "the connection's `host:` says where it is."
        }
    }

    fn not_taken(&self, span: Span, key: &str, package: &str) -> Diagnostic {
        Diagnostic::new(
            &rule::CONNECTION_KEY_INVALID,
            format!(
                "`{}` gives `{key}:`, which `{package}` doesn't take",
                self.connection
            ),
        )
        .at(span, "not taken")
    }

    fn unknown(&self, span: Span, key: &str, package: &str, reach: &Reach) -> Diagnostic {
        let mut keys: Vec<&str> = KEYS.to_vec();
        keys.extend(reach.settings.iter().map(|(setting, _)| setting.as_str()));
        let near = nearest(key, keys.iter().copied());
        let declared = if reach.settings.is_empty() {
            format!("`{package}` declares no settings of its own")
        } else {
            format!(
                "the settings `{package}` declares, {}",
                crate::and_list(
                    &reach
                        .settings
                        .iter()
                        .map(|(setting, _)| format!("`{setting}:`"))
                        .collect::<Vec<_>>()
                )
            )
        };
        let problem = Diagnostic::new(
            &rule::CONNECTION_KEY_INVALID,
            format!("`{}` has no key `{key}:`", self.connection),
        )
        .at(span.clone(), "not a connection key")
        .reason(format!(
            "A connection to a package takes {}, and {declared}.{}",
            crate::and_list(
                &KEYS
                    .iter()
                    .map(|key| format!("`{key}:`"))
                    .collect::<Vec<_>>()
            ),
            near.map_or_else(String::new, |near| format!(" Did you mean `{near}:`?"))
        ));
        match near {
            Some(near) => problem
                .next(format!("Use `{near}:` instead of `{key}:`"))
                .edit(span, near),
            None => problem.next(format!("Remove `{key}:`")),
        }
    }

    fn invalid(&self, headline: String, span: Span, label: &str) -> Diagnostic {
        Diagnostic::new(&rule::CONNECTION_VALUE_INVALID, headline).at(span, label.to_string())
    }

    /// `domains: [shipping]`: each a domain, once.
    fn domains(&self, member: &ast::Member, out: &mut Vec<Diagnostic>) -> Vec<String> {
        let Some(value) = member.value() else {
            return Vec::new();
        };
        let span = node_span(self.path, value.syntax());
        let Expr::ListExpr(list) = &value else {
            out.push(
                self.invalid(
                    format!("`{}`'s `domains:` isn't a list of domains", self.connection),
                    span,
                    "not a list",
                )
                .reason(
                    "`domains:` lists the domains whose actions may call the connection, by name.",
                )
                .next("Write `domains: [<domain>]`"),
            );
            return Vec::new();
        };
        let mut read: Vec<String> = Vec::new();
        let mut listed = 0;
        for entry in list.exprs() {
            listed += 1;
            let at = node_span(self.path, entry.syntax());
            let Expr::NameRef(name) = &entry else {
                out.push(
                    self.invalid(
                        format!(
                            "`{}`'s `domains:` lists something other than a domain",
                            self.connection
                        ),
                        at,
                        "not a domain's name",
                    )
                    .reason("`domains:` lists the domains whose actions may call the connection, by name.")
                    .next("Write each domain's name, `domains: [<domain>]`"),
                );
                continue;
            };
            let Some(domain) = tree::name_ref_text(name) else {
                continue;
            };
            if read.contains(&domain) {
                out.push(
                    self.invalid(
                        format!("`{}` lists `{domain}` twice", self.connection),
                        at.clone(),
                        "listed already",
                    )
                    .reason("`domains:` lists each domain that may call the connection once.")
                    .next(format!("Remove the second `{domain}`")),
                );
                continue;
            }
            if !self.domains.contains(&domain) {
                let near = nearest(&domain, self.domains.iter().map(String::as_str));
                let mut problem = self
                    .invalid(
                        format!(
                            "`{}` lists `{domain}`, which isn't a domain",
                            self.connection
                        ),
                        at.clone(),
                        "not a domain",
                    )
                    .reason(format!(
                        "`domains:` lists the domains whose actions may call `{}`; {}{}",
                        self.connection,
                        match self.domains {
                            [] => "the app has no domains yet.".to_string(),
                            all => format!(
                                "the app's are {}.",
                                crate::and_list(
                                    &all.iter().map(|d| format!("`{d}`")).collect::<Vec<_>>()
                                )
                            ),
                        },
                        near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                    ));
                problem = match near {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{domain}`"))
                        .edit(at, near),
                    None => problem.next(format!(
                        "List a domain the app has, or add `domains/{domain}/`"
                    )),
                };
                out.push(problem);
                continue;
            }
            read.push(domain);
        }
        // The email connection may list none: Jadpo sends the app's email
        // through it, and no domain need call it (chunk 7.3c).
        if listed == 0
            && crate::emails::written_connection(self.db, self.project).as_deref()
                != Some(self.connection)
        {
            out.push(
                self.invalid(
                    format!("`{}`'s `domains:` lists no domain", self.connection),
                    span,
                    "empty",
                )
                .reason("`domains:` lists the domains whose actions may call the connection, so with none nothing could call it.")
                .next(format!(
                    "List the domains that call `{}`, or remove the connection",
                    self.connection
                )),
            );
        }
        read
    }

    /// `config.<name>`: the setting a key reads, when it reads one.
    fn reads(&self, member: &ast::Member) -> Option<String> {
        let value = member.value()?;
        let (first, rest) = dotted(&value)?;
        if tree::name_ref_text(&first).as_deref() != Some("config") || rest.len() != 1 {
            return None;
        }
        tree::name_ref_text(&rest[0])
    }

    /// `development: { sandbox: [payment_intents.retrieve, customers] }`
    /// (chunk 7.2d): the operations and resources `jadpo dev` lets
    /// through to the sandbox, each `resource.operation` or `resource`.
    fn development(
        &self,
        member: &ast::Member,
        out: &mut Vec<Diagnostic>,
    ) -> Vec<(String, (String, u32, u32))> {
        let Some(list) = member.block() else {
            out.push(
                self.invalid(
                    format!("`{}`'s `development:` isn't a block", self.connection),
                    self.value_span(member),
                    "not a block",
                )
                .reason("`development:` says what `jadpo dev` does with the connection's calls: `sandbox:` lists the operations and resources it lets through to the provider's sandbox; every other call is faked.")
                .next("Write `development: { sandbox: [<resource>.<operation>] }`"),
            );
            return Vec::new();
        };
        let mut read = Vec::new();
        for (key, entry) in tree::members(Some(list)) {
            let Some(written) = name_text(&key) else {
                continue;
            };
            let key_span = node_span(self.path, key.syntax());
            if written != "sandbox" {
                out.push(
                    Diagnostic::new(
                        &rule::CONNECTION_KEY_INVALID,
                        format!(
                            "`{}`'s `development:` has no key `{written}:`",
                            self.connection
                        ),
                    )
                    .at(key_span, "not a development key")
                    .reason("`development:` takes `sandbox:`, the operations and resources `jadpo dev` lets through to the provider's sandbox.")
                    .next(format!("Remove `{written}:`")),
                );
                continue;
            }
            let Some(Expr::ListExpr(names)) = entry.value() else {
                out.push(
                    self.invalid(
                        format!("`{}`'s `sandbox:` isn't a list", self.connection),
                        self.value_span(&entry),
                        "not a list",
                    )
                    .reason("`sandbox:` lists the operations, `resource.operation`, and resources `jadpo dev` lets through to the provider's sandbox.")
                    .next("Write `sandbox: [<resource>.<operation>]`"),
                );
                continue;
            };
            for name in names.exprs() {
                let at = node_span(self.path, name.syntax());
                let parts = dotted(&name).map(|(first, rest)| {
                    std::iter::once(first)
                        .chain(rest)
                        .filter_map(|part| tree::name_ref_text(&part))
                        .collect::<Vec<_>>()
                });
                match parts {
                    Some(parts) if (1..=2).contains(&parts.len()) => {
                        let joined = parts.join(".");
                        if read.iter().any(|(known, _)| *known == joined) {
                            out.push(
                                self.invalid(
                                    format!("`{}`'s `sandbox:` lists `{joined}` twice", self.connection),
                                    at,
                                    "listed already",
                                )
                                .reason("`sandbox:` lists each operation or resource once.")
                                .next(format!("Remove the second `{joined}`")),
                            );
                            continue;
                        }
                        read.push((joined, (at.path, at.start, at.end)));
                    }
                    _ => out.push(
                        self.invalid(
                            format!(
                                "`{}`'s `sandbox:` lists something other than an operation or a resource",
                                self.connection
                            ),
                            at,
                            "not an operation",
                        )
                        .reason("`sandbox:` names each operation as `resource.operation` and each resource by its name, as the package's `resources/` declare them.")
                        .next("Write `sandbox: [<resource>.<operation>]`"),
                    ),
                }
            }
        }
        read
    }

    fn value_span(&self, member: &ast::Member) -> Span {
        member.value().map_or_else(
            || node_span(self.path, member.syntax()),
            |value| node_span(self.path, value.syntax()),
        )
    }

    /// What a setting of the package is given: a setting read, or a
    /// value written.
    fn given(&self, member: &ast::Member) -> Given {
        if let Some(field) = self.reads(member) {
            return Given::Setting(field);
        }
        match member.value().as_ref().map(Const::from_expr) {
            Some(Const::Str(text)) => Given::Text(text),
            Some(Const::Number {
                value,
                decimal: false,
            }) if value.scale == 0 => i64::try_from(value.mantissa)
                .map(Given::Int)
                .unwrap_or(Given::Other),
            Some(Const::Bool(flag)) => Given::Bool(flag),
            _ => Given::Other,
        }
    }

    /// An AWS credential, `credential: { key_id: config.<name>  secret:
    /// config.<name> }` (chunk 7.7c): the settings its access key id and
    /// its secret access key are read from, the secret a secret setting.
    fn aws_credential(
        &self,
        member: &ast::Member,
        out: &mut Vec<Diagnostic>,
    ) -> Option<(String, String)> {
        let shape = "`credential: { key_id: config.<name>  secret: config.<name> }`";
        let reason = "AWS signs each request with an access key id and its secret access key, so the credential gives both: `key_id:` read from a setting, and `secret:` from a secret setting (area 10, \"Secrets\").";
        let Some(block) = member.block() else {
            out.push(
                self.invalid(
                    format!(
                        "`{}`'s AWS credential needs a key id and a secret",
                        self.connection
                    ),
                    self.value_span(member),
                    "one value",
                )
                .reason(reason)
                .next(format!("Write {shape}")),
            );
            return None;
        };
        let mut key_id = None;
        let mut secret = None;
        let mut ok = true;
        for (key, entry) in tree::members(Some(block)) {
            let written = name_text(&key).unwrap_or_default();
            match written.as_str() {
                "key_id" => match self.reads(&entry) {
                    Some(field) => key_id = Some(field),
                    None => {
                        ok = false;
                        out.push(
                            self.invalid(
                                format!("`{}`'s `key_id:` isn't a setting", self.connection),
                                self.value_span(&entry),
                                "not a setting",
                            )
                            .reason(reason)
                            .next("Declare `<name>: Text` in `config.jadpo`, and write `key_id: config.<name>`"),
                        );
                    }
                },
                "secret" => match self.secret(&entry, "secret access key", out) {
                    Some(field) => secret = Some(field),
                    None => ok = false,
                },
                other => {
                    ok = false;
                    out.push(
                        Diagnostic::new(
                            &rule::CONNECTION_KEY_INVALID,
                            format!("`{}`'s credential has no key `{other}:`", self.connection),
                        )
                        .at(node_span(self.path, key.syntax()), "not a credential key")
                        .reason(reason)
                        .next(format!("Remove `{other}:`")),
                    );
                }
            }
        }
        match (key_id, secret, ok) {
            (Some(key_id), Some(secret), true) => Some((key_id, secret)),
            (key_id, secret, true) => {
                let missing = if key_id.is_none() { "key_id" } else { "secret" };
                let _ = secret;
                out.push(
                    Diagnostic::new(
                        &rule::CONNECTION_KEY_MISSING,
                        format!(
                            "`{}`'s credential doesn't give `{missing}:`",
                            self.connection
                        ),
                    )
                    .at(
                        node_span(self.path, member.syntax()),
                        format!("no `{missing}:`"),
                    )
                    .reason(reason)
                    .next(format!("Write {shape}")),
                );
                None
            }
            _ => None,
        }
    }

    /// `credential: config.<name>`, a secret setting.
    fn credential(&self, member: &ast::Member, out: &mut Vec<Diagnostic>) -> Option<String> {
        let span = self.value_span(member);
        let headline = format!(
            "`{}`'s credential must be a secret setting",
            self.connection
        );
        let reason = "A credential is read from the deployment's environment and never seen by code, so it is a `secret: true` setting, `config.<name>` (area 10, \"Secrets\").";
        let Some(field) = self.reads(member) else {
            out.push(
                self.invalid(headline, span, "a credential written in source")
                    .reason(reason)
                    .next("Declare `<name>: Text { secret: true }` in `config.jadpo`, and write `credential: config.<name>`"),
            );
            return None;
        };
        // An unknown or unreadable setting is reported where it is read.
        let setting = config(self.db, self.project).setting(&field)?;
        if !setting.secret {
            out.push(
                self.invalid(headline, span, format!("`{field}` isn't secret").as_str())
                    .reason(reason)
                    .next(format!("Add `secret: true` to `{field}` in `config.jadpo`")),
            );
            return None;
        }
        Some(field)
    }

    /// `webhooks: { secret: config.<name>  replies: { } }` (chunk 7.7):
    /// the secret setting the provider's webhooks are checked with, and
    /// where it is written; and the actions that answer the webhooks that
    /// need a reply, as written (chunk 7.7b).
    fn webhooks(
        &self,
        member: &ast::Member,
        out: &mut Vec<Diagnostic>,
    ) -> (Option<WrittenSecret>, Vec<WrittenReply>) {
        let Some(block) = member.block() else {
            out.push(
                self.invalid(
                    format!("`{}`'s `webhooks:` isn't a block", self.connection),
                    self.value_span(member),
                    "not a block",
                )
                .reason("`webhooks:` gives the secret the provider signs its webhooks with, from a secret setting, which turns on the route that receives them.")
                .next("Write `webhooks: { secret: config.<name> }`"),
            );
            return (None, Vec::new());
        };
        let mut secret = None;
        let mut replies = Vec::new();
        let mut given = false;
        for (key, entry) in tree::members(Some(block)) {
            let Some(written) = name_text(&key) else {
                continue;
            };
            let key_span = node_span(self.path, key.syntax());
            match written.as_str() {
                "secret" => {
                    given = true;
                    secret = self
                        .secret(&entry, "webhook secret", out)
                        .map(|field| (field, (key_span.path, key_span.start, key_span.end)));
                }
                "replies" => replies = self.replies(&entry, out),
                other => out.push(
                    Diagnostic::new(
                        &rule::CONNECTION_KEY_INVALID,
                        format!("`{}`'s `webhooks:` has no key `{other}:`", self.connection),
                    )
                    .at(key_span.clone(), "not a webhooks key")
                    .reason("`webhooks:` takes `secret:`, the secret setting the provider's webhooks are checked with, and `replies:`, the actions that answer the webhooks that need a reply.")
                    .next(match nearest(other, ["secret", "replies"]) {
                        Some(near) => format!("Use `{near}:` instead of `{other}:`"),
                        None => format!("Remove `{other}:`"),
                    }),
                ),
            }
        }
        if !given {
            out.push(
                Diagnostic::new(
                    &rule::CONNECTION_KEY_MISSING,
                    format!("`{}`'s `webhooks:` doesn't give `secret:`", self.connection),
                )
                .at(node_span(self.path, member.syntax()), "no `secret:`")
                .reason("The runtime checks each webhook's signature with the secret the provider gave, read from a secret setting, before anything in it is kept.")
                .next("Declare `<name>: Text { secret: true }` in `config.jadpo`, and add `secret: config.<name>`"),
            );
        }
        (secret, replies)
    }

    /// `replies: { <event>: <domain>.<action> }` as written (chunk 7.7b).
    fn replies(&self, member: &ast::Member, out: &mut Vec<Diagnostic>) -> Vec<WrittenReply> {
        let Some(block) = member.block() else {
            out.push(
                self.invalid(
                    format!("`{}`'s `replies:` isn't a block", self.connection),
                    self.value_span(member),
                    "not a block",
                )
                .reason("`replies:` names, for each webhook event that needs a reply, the app's action that answers it, keyed by the event's name in lower_snake_case.")
                .next("Write `replies: { <event>: <domain>.<action> }`"),
            );
            return Vec::new();
        };
        tree::members(Some(block))
            .into_iter()
            .filter_map(|(key, entry)| {
                let event = name_text(&key)?;
                let action =
                    entry
                        .value()
                        .as_ref()
                        .and_then(tree::dotted)
                        .and_then(|(first, rest)| {
                            std::iter::once(&first)
                                .chain(&rest)
                                .map(tree::name_ref_text)
                                .collect::<Option<Vec<String>>>()
                        });
                let key_span = node_span(self.path, key.syntax());
                let value_span = self.value_span(&entry);
                Some(WrittenReply {
                    event,
                    key_at: (key_span.path, key_span.start, key_span.end),
                    action,
                    value_at: (value_span.path, value_span.start, value_span.end),
                })
            })
            .collect()
    }

    /// A key read from a secret setting, `config.<name>`: `what` names it
    /// for a problem.
    fn secret(
        &self,
        member: &ast::Member,
        what: &str,
        out: &mut Vec<Diagnostic>,
    ) -> Option<String> {
        let span = self.value_span(member);
        let headline = format!("`{}`'s {what} must be a secret setting", self.connection);
        let reason = format!(
            "A {what} is read from the deployment's environment and never seen by code, so it is a `secret: true` setting, `config.<name>` (area 10, \"Secrets\")."
        );
        let Some(field) = self.reads(member) else {
            out.push(
                self.invalid(headline, span, format!("a {what} written in source").as_str())
                    .reason(reason)
                    .next("Declare `<name>: Text { secret: true }` in `config.jadpo`, and read it as `config.<name>`"),
            );
            return None;
        };
        let setting = config(self.db, self.project).setting(&field)?;
        if !setting.secret {
            out.push(
                self.invalid(headline, span, format!("`{field}` isn't secret").as_str())
                    .reason(reason)
                    .next(format!("Add `secret: true` to `{field}` in `config.jadpo`")),
            );
            return None;
        }
        Some(field)
    }

    /// `environment: config.<name>`, a `ProviderEnvironment` setting.
    fn environment(&self, member: &ast::Member, out: &mut Vec<Diagnostic>) -> Option<String> {
        let span = self.value_span(member);
        let headline = format!(
            "`{}`'s environment must be a `ProviderEnvironment` setting",
            self.connection
        );
        let reason = "`environment:` chooses the package's live or sandbox host for each deployment, so it is read from a setting, `config.<name>`, a `ProviderEnvironment` (`.sandbox` or `.live`).";
        let next = "Declare `<name>: ProviderEnvironment` in `config.jadpo`, and write `environment: config.<name>`";
        let Some(field) = self.reads(member) else {
            out.push(
                self.invalid(headline, span, "written in source")
                    .reason(reason)
                    .next(next),
            );
            return None;
        };
        let setting = config(self.db, self.project).setting(&field)?;
        if setting.ty.to_string() != jadpo_std::ProviderEnvironment::NAME {
            out.push(
                self.invalid(
                    headline,
                    span,
                    format!("`{field}` is `{}`", setting.ty).as_str(),
                )
                .reason(reason)
                .next(next),
            );
            return None;
        }
        Some(field)
    }

    /// `host: config.<name>`, a `Url` setting.
    fn host(&self, member: &ast::Member, out: &mut Vec<Diagnostic>) -> Option<String> {
        let span = self.value_span(member);
        let headline = format!("`{}`'s host must be a `Url` setting", self.connection);
        let reason = "A package with no fixed hosts is reached at the address each deployment gives, so it is read from a setting, `config.<name>`, a `Url`.";
        let next = "Declare `<name>: Url` in `config.jadpo`, and write `host: config.<name>`";
        let Some(field) = self.reads(member) else {
            out.push(
                self.invalid(headline, span, "an address written in source")
                    .reason(reason)
                    .next(next),
            );
            return None;
        };
        let setting = config(self.db, self.project).setting(&field)?;
        if Lookup::new(self.db, self.project).base_prim(&setting.ty) != Some(Prim::Url) {
            out.push(
                self.invalid(
                    headline,
                    span,
                    format!("`{field}` is `{}`", setting.ty).as_str(),
                )
                .reason(reason)
                .next(next),
            );
            return None;
        }
        Some(field)
    }

    /// `timeout: 5s`: how long a call waits for its answer, a duration
    /// written out (chunk 7.2c).
    /// `max_concurrent: 2` (chunk 7.6c): a whole number of at least 1,
    /// and no more than its package's, which it may only lower.
    fn max_concurrent(
        &self,
        member: &ast::Member,
        reach: &Reach,
        out: &mut Vec<Diagnostic>,
    ) -> Option<u32> {
        let given = whole(member).filter(|n| reach.max_concurrent.is_none_or(|most| *n <= most));
        if given.is_none() {
            let range = match reach.max_concurrent {
                Some(most) => format!("from 1 to {most}"),
                None => "of at least 1".to_string(),
            };
            out.push(
                self.invalid(
                    format!(
                        "`{}`'s `max_concurrent:` must be a whole number {range}",
                        self.connection
                    ),
                    self.value_span(member),
                    &format!("not {range}"),
                )
                .reason(match reach.max_concurrent {
                    Some(most) => format!(
                        "`max_concurrent:` caps how many calls through the connection go out at once per app instance; its package allows {most}, and a connection may only lower that."
                    ),
                    None => "`max_concurrent:` caps how many calls through the connection go out at once per app instance, a whole number such as `2`; further calls wait.".to_string(),
                })
                .next(match reach.max_concurrent {
                    Some(most) => format!("Write a number from 1 to {most}, or remove `max_concurrent:` to allow {most}"),
                    None => "Write a whole number such as `max_concurrent: 2`, or remove it".to_string(),
                }),
            );
        }
        given
    }

    /// `polls: { latest: 6h }` (chunk 7.6c): each one of the package's
    /// polls, `names`, given a duration of at least a minute.
    fn polls(
        &self,
        member: &ast::Member,
        names: &[String],
        out: &mut Vec<Diagnostic>,
    ) -> Vec<(String, u64)> {
        let mut read = Vec::new();
        let Some(block) = member.block() else {
            out.push(
                self.invalid(
                    format!("`{}`'s `polls:` holds no block", self.connection),
                    self.value_span(member),
                    "not a block",
                )
                .reason("`polls:` gives each of the package's polls the connection runs at another interval, by its name: `polls: { latest: 6h }`.")
                .next("Write `polls: { <poll>: <interval> }`, or remove it"),
            );
            return read;
        };
        let known: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
        for (key, entry) in tree::members(Some(block)) {
            let name = name_text(&key).unwrap_or_default();
            let key_span = node_span(self.path, key.syntax());
            if !names.contains(&name) {
                let near = nearest(&name, names.iter().map(String::as_str));
                let problem = Diagnostic::new(
                    &rule::CONNECTION_KEY_INVALID,
                    format!("`{}`'s `polls:` has no poll `{name}`", self.connection),
                )
                .at(key_span.clone(), "no such poll")
                .reason(format!(
                    "`polls:` changes how often the package's polls run, each by its name in the package's `provider.jadpo`: {}.",
                    crate::and_list(&known)
                ));
                out.push(match near {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{name}`"))
                        .edit(key_span, near),
                    None => problem.next(format!("Remove `{name}:`")),
                });
                continue;
            }
            let millis = match entry.value().as_ref().map(Const::from_expr) {
                Some(Const::Duration(text)) => jadpo_std::Dur::parse(&text)
                    .and_then(|dur| u64::try_from(dur.millis).ok())
                    .filter(|millis| *millis >= crate::polls::LEAST_EVERY_MS),
                _ => None,
            };
            match millis {
                Some(millis) => read.push((name, millis)),
                None => out.push(
                    self.invalid(
                        format!(
                            "`{}`'s `polls: {{ {name}: }}` must be a duration of at least a minute",
                            self.connection
                        ),
                        self.value_span(&entry),
                        "not a minute or more",
                    )
                    .reason("A poll is called by the app's tick, once a minute at most, so it runs at most once a minute: its interval is a duration such as `6h`.")
                    .next(format!("Write `{name}: 1m` or longer, or remove it for the package's own interval")),
                ),
            }
        }
        read
    }

    fn timeout(&self, member: &ast::Member, out: &mut Vec<Diagnostic>) -> Option<u64> {
        let span = self.value_span(member);
        let given = match member.value().as_ref().map(Const::from_expr) {
            Some(Const::Duration(text)) => jadpo_std::Dur::parse(&text)
                .and_then(|dur| u64::try_from(dur.millis).ok())
                .filter(|millis| *millis > 0),
            _ => None,
        };
        if given.is_none() {
            out.push(
                self.invalid(
                    format!("`{}`'s `timeout:` must be a duration", self.connection),
                    span,
                    "not a duration",
                )
                .reason("`timeout:` is how long each call through the connection waits for its answer before it is given up, a duration written out such as `5s`; without it a call waits 10 seconds.")
                .next("Write `timeout: 5s`, or remove it to wait 10 seconds"),
            );
        }
        given
    }

    /// `retry: { attempts: 3 within: 30s }` (chunk 7.2c2): how many times
    /// a call that may be sent again is tried, the first included, from 1
    /// to [`jadpo_std::provider::MAX_ATTEMPTS`], and within how long of the
    /// first, a duration up to [`jadpo_std::provider::MAX_WITHIN_MS`]; each
    /// left out keeps its default.
    fn retry(
        &self,
        member: &ast::Member,
        out: &mut Vec<Diagnostic>,
    ) -> Option<jadpo_std::provider::RetryPlan> {
        use jadpo_std::provider::{DEFAULT_RETRY, MAX_ATTEMPTS, MAX_WITHIN_MS};
        let reason = format!(
            "`retry:` holds `attempts:`, how many times a call that may be sent again is tried, from 1 to {MAX_ATTEMPTS} (3 by default), and `within:`, a duration up to 5 minutes after the first attempt (30 seconds by default)."
        );
        let next =
            "Write `retry: { attempts: 3 within: 30s }`, or leave `retry:` out for the default";
        let Some(block) = member.block() else {
            out.push(
                self.invalid(
                    format!("`{}`'s `retry:` holds no block", self.connection),
                    self.value_span(member),
                    "not a block",
                )
                .reason(reason)
                .next(next),
            );
            return None;
        };
        let mut retry = DEFAULT_RETRY;
        let before = out.len();
        for (setting, entry) in tree::members(Some(block)) {
            let name = name_text(&setting).unwrap_or_default();
            let span = self.value_span(&entry);
            let value = entry.value().as_ref().map(Const::from_expr);
            match name.as_str() {
                "attempts" => match value {
                    Some(Const::Number {
                        value,
                        decimal: false,
                    }) if value.mantissa >= 1 && value.mantissa <= i128::from(MAX_ATTEMPTS) => {
                        retry.attempts = u32::try_from(value.mantissa).unwrap_or(1);
                    }
                    _ => out.push(
                        self.invalid(
                            format!(
                                "`{}`'s `retry: {{ attempts: }}` must be from 1 to {MAX_ATTEMPTS}",
                                self.connection
                            ),
                            span,
                            &format!("not 1 to {MAX_ATTEMPTS}"),
                        )
                        .reason(reason.clone())
                        .next(next),
                    ),
                },
                "within" => {
                    let millis = match value {
                        Some(Const::Duration(text)) => jadpo_std::Dur::parse(&text)
                            .and_then(|dur| u64::try_from(dur.millis).ok())
                            .filter(|millis| *millis > 0 && *millis <= MAX_WITHIN_MS),
                        _ => None,
                    };
                    match millis {
                        Some(millis) => retry.within_ms = millis,
                        None => out.push(
                            self.invalid(
                                format!("`{}`'s `retry: {{ within: }}` must be a duration up to 5 minutes", self.connection),
                                span,
                                "not a duration up to 5 minutes",
                            )
                            .reason(reason.clone())
                            .next(next),
                        ),
                    }
                }
                other => out.push(
                    self.invalid(
                        format!("`{other}` isn't part of `retry:`"),
                        node_span(self.path, setting.syntax()),
                        "not a retry setting",
                    )
                    .reason(reason.clone())
                    .next(next),
                ),
            }
        }
        (out.len() == before).then_some(retry)
    }

    /// A setting the package declares: a value of its type, or a setting
    /// of it.
    fn setting(
        &self,
        name: &str,
        prim: Prim,
        package: &str,
        member: &ast::Member,
        out: &mut Vec<Diagnostic>,
    ) {
        let span = self.value_span(member);
        let fits = match self.reads(member) {
            Some(field) => match config(self.db, self.project).setting(&field) {
                Some(setting) => {
                    Lookup::new(self.db, self.project).base_prim(&setting.ty) == Some(prim)
                }
                None => true,
            },
            None => match member.value().as_ref().map(Const::from_expr) {
                Some(Const::Str(_)) => prim == Prim::Text,
                Some(Const::Number { decimal, .. }) => {
                    prim == Prim::Decimal || (prim == Prim::Int && !decimal)
                }
                Some(Const::Bool(_)) => prim == Prim::Bool,
                Some(Const::Duration(_)) => prim == Prim::Duration,
                _ => false,
            },
        };
        if fits {
            return;
        }
        out.push(
            self.invalid(
                format!("`{}`'s `{name}:` must be a `{}`", self.connection, prim.name()),
                span,
                format!("not a `{}`", prim.name()).as_str(),
            )
            .reason(format!(
                "`{package}` declares `{name}:` as a `{}`, which a connection gives as a value of that type or reads from a setting of it, `config.<name>`.",
                prim.name()
            ))
            .next(format!("Give `{name}:` a `{}` value", prim.name())),
        );
    }
}

// --- the network report ------------------------------------------------------

/// The network report (area 11, question 9): every host the app can
/// reach, by the connection that reaches it, as Markdown and as JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub markdown: String,
    pub json: Value,
}

/// One row: a host one connection reaches.
struct Row {
    connection: String,
    /// The host as the report shows it.
    host: String,
    /// The same, as JSON gives it: an address, or the setting it is read
    /// from, or the service's image.
    address: Option<String>,
    setting: Option<String>,
    image: Option<String>,
    /// When it is reached: `None` always, or the environment's setting
    /// and value.
    when: Option<(String, &'static str)>,
    /// What it is reached for: a package, or a capability.
    reached_for: String,
    domains: Vec<String>,
}

/// The report, once the connections read cleanly.
pub fn report(db: &dyn Db, project: Project) -> Report {
    let mut rows: Vec<Row> = Vec::new();
    for connection in &package_connections(db, project).connections {
        let package = format!("`{}`", connection.package);
        let reach = &connection.reach;
        let base = |host: String| Row {
            connection: connection.name.clone(),
            host,
            address: None,
            setting: None,
            image: None,
            when: None,
            reached_for: package.clone(),
            domains: connection.domains.clone(),
        };
        if reach.hosts {
            for (address, word) in [(&reach.live, "live"), (&reach.sandbox, "sandbox")] {
                let Some(address) = address else {
                    continue;
                };
                let mut row = base(format!("`{address}`"));
                row.address = Some(address.clone());
                row.when = connection
                    .environment
                    .as_ref()
                    .map(|setting| (setting.clone(), word));
                rows.push(row);
            }
        } else if let Some(setting) = &connection.host {
            let mut row = base(format!("the address in `config.{setting}`"));
            row.setting = Some(setting.clone());
            rows.push(row);
        } else if reach.runs_written {
            let image = reach.runs.clone().unwrap_or_default();
            let mut row = base(format!("the service Jadpo runs from `{image}`"));
            row.image = Some(image);
            rows.push(row);
        }
    }
    // The driver connections whose address a setting gives; a database
    // says how it is secured unless it is the default (chunk 8.1n).
    let model = crate::data::data_model(db, project);
    let databases = crate::databases::database_connections(db, project);
    for url in &connection_urls(db, project).urls {
        let mut domains: Vec<String> = model
            .entities
            .iter()
            .filter(|entity| entity.store.as_deref() == Some(url.connection.as_str()))
            .filter_map(|entity| {
                let rest = entity.path.strip_prefix("domains/")?;
                Some(rest.split('/').next()?.to_string())
            })
            .collect();
        domains.sort();
        domains.dedup();
        rows.push(Row {
            connection: url.connection.clone(),
            host: format!("the address in `config.{}`", url.setting),
            address: None,
            setting: Some(url.setting.clone()),
            image: None,
            when: None,
            reached_for: match url.kind {
                ConnectionKind::Redis => "cache".to_string(),
                _ => match databases.tls_of(&url.connection) {
                    crate::databases::DatabaseTls::Roots => "database".to_string(),
                    crate::databases::DatabaseTls::Authority(setting) => {
                        format!("database, its certificate checked against `config.{setting}`")
                    }
                    crate::databases::DatabaseTls::Off => "database, without TLS".to_string(),
                },
            },
            domains,
        });
    }
    // Each SMTP server, which email is reached for (area 16; chunk 7.3),
    // by the domains whose emails go through it, the one `email:` names.
    let sending: Vec<String> = {
        let mut domains: Vec<String> = crate::emails::analysis(db, project)
            .emails
            .iter()
            .filter_map(|email| email.ownership.domain().map(str::to_string))
            .collect();
        domains.sort();
        domains.dedup();
        domains
    };
    let mailer = crate::emails::settings(db, project).connection.clone();
    for smtp in &crate::emails::smtp_connections(db, project).list {
        let (host, address, setting) = match &smtp.host {
            Some(crate::emails::SmtpHost::Written(host)) => (
                format!("`{host}:{}`", smtp.port),
                Some(format!("{host}:{}", smtp.port)),
                None,
            ),
            Some(crate::emails::SmtpHost::Setting(name)) => (
                format!("the server in `config.{name}`, port {}", smtp.port),
                None,
                Some(name.clone()),
            ),
            None => continue,
        };
        rows.push(Row {
            connection: smtp.name.clone(),
            host,
            address,
            setting,
            image: None,
            when: None,
            reached_for: "email".to_string(),
            domains: if mailer.as_deref() == Some(smtp.name.as_str()) {
                sending.clone()
            } else {
                Vec::new()
            },
        });
    }
    // The model `ai:` names (area 21; chunk 5.5v3), by the domains whose
    // actions ask it.
    let asking = crate::ai::asking_domains(db, project);
    let named = crate::ai::settings(db, project).connection.clone();
    for model in &crate::ai::model_connections(db, project).list {
        let (host, address, setting) = match &model.url {
            Some(crate::ai::ModelUrl::Written(url)) => {
                (format!("`{url}`"), Some(url.clone()), None)
            }
            Some(crate::ai::ModelUrl::Setting(name)) => (
                format!("the address in `config.{name}`"),
                None,
                Some(name.clone()),
            ),
            None => continue,
        };
        rows.push(Row {
            connection: model.name.clone(),
            host,
            address,
            setting,
            image: None,
            when: None,
            reached_for: "ai".to_string(),
            domains: if named.as_deref() == Some(model.name.as_str()) {
                asking.clone()
            } else {
                Vec::new()
            },
        });
    }
    let identity = crate::identity::identity(db, project);
    if let Some(external) = &identity.external_token {
        rows.push(Row {
            connection: "external_token".into(),
            host: format!("the issuer in `config.{}` and the public HTTPS keys address its verified discovery document names", external.issuer),
            address: None, setting: Some(external.issuer.clone()), image: None,
            when: None, reached_for: "runtime identity discovery and signing keys; public HTTPS, no redirects".into(), domains: Vec::new(),
        });
    }
    rows.sort_by(|a, b| (&a.connection, &a.host).cmp(&(&b.connection, &b.host)));
    let mut markdown = String::from(
        "# Network\n\n\
         Every host the app can reach, and the connection that reaches it. Jadpo code reaches \
         the network only through connections in `app.jadpo`. Written by `jadpo check`.\n\n",
    );
    if identity.external_token.is_some() {
        markdown.push_str("External identity discovery is runtime-owned egress: only the configured issuer and its exactly matched discovery document's public HTTPS keys address, with certificate checks and no redirects. Token headers never choose an address.\n\n");
    }
    if rows.is_empty() {
        markdown.push_str("The app reaches no host.\n");
    } else {
        markdown.push_str("| Connection | Host | When | For | Domains |\n|---|---|---|---|---|\n");
        for row in &rows {
            let when = match &row.when {
                Some((setting, word)) => format!("`config.{setting}` is `{word}`"),
                None => "always".to_string(),
            };
            let domains = if row.domains.is_empty() {
                "none".to_string()
            } else {
                row.domains
                    .iter()
                    .map(|domain| format!("`{domain}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            markdown.push_str(&format!(
                "| `{}` | {} | {when} | {} | {domains} |\n",
                row.connection, row.host, row.reached_for
            ));
        }
    }
    let mut hosts = Vec::new();
    for row in &rows {
        let mut entry = Value::object();
        entry.set("connection", row.connection.as_str());
        if let Some(address) = &row.address {
            entry.set("address", address.as_str());
        }
        if let Some(setting) = &row.setting {
            entry.set("setting", setting.as_str());
        }
        if let Some(image) = &row.image {
            entry.set("service", image.as_str());
        }
        if let Some((setting, word)) = &row.when {
            let mut when = Value::object();
            when.set("setting", setting.as_str());
            when.set("is", *word);
            entry.set("when", when);
        }
        match row.reached_for.split_once(", ") {
            Some((reached_for, _)) if row.reached_for.starts_with("database, ") => {
                entry.set("for", reached_for);
                match databases.tls_of(&row.connection) {
                    crate::databases::DatabaseTls::Authority(setting) => {
                        entry.set("authority", setting.as_str());
                    }
                    crate::databases::DatabaseTls::Off => entry.set("tls", false),
                    crate::databases::DatabaseTls::Roots => {}
                }
            }
            _ => entry.set("for", row.reached_for.trim_matches('`')),
        }
        entry.set("domains", row.domains.clone());
        hosts.push(entry);
    }
    let mut json = Value::object();
    json.set("hosts", hosts);
    if let Some(external) = &identity.external_token {
        let mut rule = Value::object();
        rule.set("issuer_setting", external.issuer.as_str());
        rule.set("audience_setting", external.audience.as_str());
        rule.set("public_https", true);
        rule.set("redirects", false);
        rule.set(
            "keys_authority",
            "exactly matched configured issuer discovery metadata",
        );
        json.set("external_identity", rule);
    }
    Report { markdown, json }
}
