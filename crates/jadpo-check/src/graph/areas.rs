//! The nodes of areas 8, 10 and 11 that `app.jadpo`, `config.jadpo`,
//! `authentication.jadpo` and the lock declare rather than a file of their
//! own (docs/compiler-runtime.md §2.3, "Outside, access and services";
//! chunk 10.3g): each connection, the hosts the network report lists and
//! each lock line; each kind of caller, sign-in method, way of signing in
//! and route the runtime answers itself; each setting, the locale set and
//! the rate limits every route is counted against.
//!
//! Each is read from the checker's own model of it, placed and digested at
//! what is written for it, so an edit elsewhere in its file leaves its
//! digest alone. Packages, their resources, operations and capability
//! mappings, schedules and subscribers are declarations, and are their
//! files' parts ([`super::part`]).

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Project};
use jadpo_graph::{Detail, Digest, EdgeKind, Key, Kind, Node, Place};
use jadpo_std::http::{ADDRESS_LIMIT, CALLER_LIMIT, RateLimit};
use jadpo_syntax::SyntaxNode;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::text::{LineIndex, digest, doc, place};
use super::{declaration_key, entity_field_key, registry, type_ref};
use crate::config::singletons;
use crate::data::ConnectionKind;
use crate::lookups::{Lookup, Name};
use crate::tree::{self, dotted, name_ref_text, name_text};

/// Nodes, and the edges that start at them.
pub type Part = (Vec<Node>, Vec<(Key, EdgeKind, Key)>);

/// What is written for one node: a member of a declaration that exists
/// once (`application { connections: { couriers: { } } }`), in its file.
struct Written {
    path: String,
    syntax: SyntaxNode,
}

/// The member of `keyword { }` that `names` lead to.
fn written(db: &dyn Db, project: Project, keyword: &str, names: &[&str]) -> Option<Written> {
    singletons(db, project, keyword)
        .into_iter()
        .find_map(|found| {
            let mut list = found.list.clone();
            let mut member = None;
            for name in names {
                let (_, next) = tree::members(list)
                    .into_iter()
                    .find(|(key, _)| name_text(key).as_deref() == Some(*name))?;
                list = next.block();
                member = Some(next);
            }
            Some(Written {
                path: found.path.clone(),
                syntax: member?.syntax().clone(),
            })
        })
}

/// The whole of `keyword { }`.
fn declared(db: &dyn Db, project: Project, keyword: &str) -> Option<Written> {
    let found = singletons(db, project, keyword).into_iter().next()?;
    let syntax = found.list.as_ref()?.syntax().parent()?;
    Some(Written {
        path: found.path,
        syntax,
    })
}

/// A node placed and digested at what is written for it, with its `///`
/// description.
fn node_at(db: &dyn Db, project: Project, key: Key, kind: Kind, at: &Written) -> Node {
    let name = key.name().to_string();
    let mut node = Node::new(key, kind, name);
    if let Some(file) = crate::lookups::file_at(db, project, Name::new(db, at.path.clone())) {
        node.place = Some(place(&at.path, &LineIndex::new(file.text(db)), &at.syntax));
    }
    node.digest = Some(digest(&at.syntax));
    node.doc = doc(&at.syntax);
    node
}

/// A node no source writes, digested from what it holds.
fn node_of(key: Key, kind: Kind, parts: &[&str]) -> Node {
    let name = key.name().to_string();
    let mut node = Node::new(key, kind, name);
    node.digest = Some(Digest::of(parts.iter().copied()));
    node
}

/// The settings `syntax` reads, `config.<name>`, as their nodes' keys.
fn settings_read(db: &dyn Db, project: Project, syntax: &SyntaxNode) -> BTreeSet<Key> {
    let registry = registry(db, project);
    syntax
        .descendants()
        .filter_map(Expr::cast)
        .filter_map(|expr| {
            let (first, rest) = dotted(&expr)?;
            let [name] = rest.as_slice() else {
                return None;
            };
            (name_ref_text(&first).as_deref() == Some(crate::config::CONFIG))
                .then(|| name_ref_text(name))
                .flatten()
        })
        .map(|name| registry.key(Kind::ConfigField, &name, None))
        .collect()
}

fn uses(edges: &mut Vec<(Key, EdgeKind, Key)>, from: &Key, to: impl IntoIterator<Item = Key>) {
    for to in to {
        edges.push((from.clone(), EdgeKind::Uses, to));
    }
}

/// The connections whose package is a provider's, by name, each with the
/// name the package's declarations are written through (`SwiftCouriers`),
/// which its operations are keyed by.
#[salsa::tracked(returns(ref))]
pub fn connection_packages(db: &dyn Db, project: Project) -> BTreeMap<String, String> {
    let homes = crate::surface::homes(db, project);
    crate::connections::package_connections(db, project)
        .connections
        .iter()
        .filter_map(|connection| {
            let home = homes.get(&connection.folder)?;
            Some((connection.name.clone(), home.through.clone()))
        })
        .collect()
}

/// The key of a provider package's operation, `resource.operation` of the
/// package written through `through`.
pub fn operation_key(db: &dyn Db, project: Project, through: &str, operation: &str) -> Key {
    registry(db, project).key(Kind::Operation, &format!("{through}.{operation}"), None)
}

/// What a connection's driver serves, as its node says.
fn capability(kind: ConnectionKind) -> &'static str {
    match kind {
        ConnectionKind::Postgres => "database",
        ConnectionKind::Redis => "cache",
        ConnectionKind::Files => "files",
        ConnectionKind::Smtp => "email",
        ConnectionKind::Model => "ai",
        ConnectionKind::CloudflareQueue => "receiving",
        ConnectionKind::CloudflareRateLimits => "rate_limits",
        ConnectionKind::SignIn => "sign_in",
        ConnectionKind::Package => "package",
        ConnectionKind::Other => "other",
    }
}

/// Area 11's nodes outside packages: each connection, with `uses` edges
/// to its package and the settings it reads; each host the network report
/// lists, under its connection; and each line of the lock, under its
/// package.
#[salsa::tracked(returns(ref))]
pub fn outside_nodes(db: &dyn Db, project: Project) -> Part {
    let registry = registry(db, project);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let connections = crate::data::connections(db, project);
    let packaged = crate::connections::package_connections(db, project);
    let homes = crate::surface::homes(db, project);
    let mut declared: Vec<(String, ConnectionKind)> = connections.declared.clone();
    // `main` on PostgreSQL, implied while no connection is a database
    // and something is stored.
    if connections.implied_main
        && crate::data::schema(db, project)
            .stores
            .contains_key(crate::data::MAIN)
    {
        declared.push((crate::data::MAIN.to_string(), ConnectionKind::Postgres));
    }
    let mut names = BTreeSet::new();
    for (name, kind) in declared {
        if !names.insert(name.clone()) {
            continue;
        }
        let key = registry.key(Kind::Connection, &name, None);
        let at = written(db, project, "application", &["connections", &name]);
        let mut node = match &at {
            Some(at) => node_at(db, project, key.clone(), Kind::Connection, at),
            None => node_of(key.clone(), Kind::Connection, &[&name, capability(kind)]),
        };
        let package = packaged
            .connections
            .iter()
            .find(|connection| connection.name == name);
        node.detail = Detail::Connection {
            capability: capability(kind).to_string(),
            package: package.map(|connection| connection.package.clone()),
            domains: package.map_or_else(Vec::new, |connection| connection.domains.clone()),
            credential: package.and_then(|connection| connection.reach.credential.clone()),
        };
        if let Some(home) = package.and_then(|connection| homes.get(&connection.folder)) {
            uses(
                &mut edges,
                &key,
                [registry.key(Kind::Package, &home.through, None)],
            );
        }
        if let Some(at) = &at {
            uses(&mut edges, &key, settings_read(db, project, &at.syntax));
        }
        nodes.push(node);
    }
    // Each host, keyed by its connection and, where a setting chooses
    // between them, the environment it is reached in: its address is what
    // it holds, so a changed address reads as a change, not a new host.
    for row in crate::connections::hosts(db, project, false) {
        let name = match &row.when {
            Some((_, word)) => format!("{}.{word}", row.connection),
            None => row.connection.clone(),
        };
        let key = registry.key(Kind::Host, &name, None);
        let when = row
            .when
            .as_ref()
            .map(|(setting, word)| format!("{setting}={word}"));
        let mut node = node_of(
            key.clone(),
            Kind::Host,
            &[
                row.address.as_deref().unwrap_or_default(),
                row.setting.as_deref().unwrap_or_default(),
                row.image.as_deref().unwrap_or_default(),
                when.as_deref().unwrap_or_default(),
                &row.reached_for,
            ],
        );
        if names.contains(&row.connection) {
            let connection = registry.key(Kind::Connection, &row.connection, None);
            node.place = nodes
                .iter()
                .find(|node: &&Node| node.key == connection)
                .and_then(|node| node.place.clone());
            node.parent = Some(connection);
        }
        node.detail = Detail::Host {
            address: row.address.clone(),
            setting: row.setting.clone(),
            service: row.image.clone(),
            environment: row
                .when
                .as_ref()
                .map(|(setting, word)| (setting.clone(), word.to_string())),
            reached_for: row.reached_for.trim_matches('`').to_string(),
        };
        let read = row
            .setting
            .iter()
            .map(|setting| registry.key(Kind::ConfigField, setting, None));
        uses(&mut edges, &key, read);
        nodes.push(node);
    }
    nodes.extend(lock_entries(db, project));
    (nodes, edges)
}

/// Each line of `jadpo.lock` (area 18, "The lock"), placed at its line,
/// under the package it locks.
fn lock_entries(db: &dyn Db, project: Project) -> Vec<Node> {
    use jadpo_db::Source;
    let registry = registry(db, project);
    let Some((path, lines)) = crate::packages::lock(db, project) else {
        return Vec::new();
    };
    let text = project
        .records(db)
        .iter()
        .find(|record| record.path == path)
        .and_then(|record| record.text.as_ref().ok())
        .map(String::as_str)
        .unwrap_or_default();
    let index = LineIndex::new(text);
    let homes = crate::surface::homes(db, project);
    let mut out = Vec::new();
    for locked in lines {
        let line = locked.line();
        let key = registry.key(Kind::LockEntry, &locked.package, None);
        let mut node = node_of(key, Kind::LockEntry, &[&line]);
        if let Some(start) = text
            .match_indices(&line)
            .map(|(at, _)| at)
            .find(|&at| at == 0 || text.as_bytes()[at - 1] == b'\n')
        {
            let start = u32::try_from(start).unwrap_or(u32::MAX);
            let end = start.saturating_add(u32::try_from(line.len()).unwrap_or(u32::MAX));
            node.place = Some(Place {
                file: path.clone(),
                start,
                end,
                lines: (index.line(start), index.line(start)),
            });
        }
        node.parent = homes
            .get(&locked.folder())
            .map(|home| registry.key(Kind::Package, &home.through, None));
        let (version, source, location, tag, commit, digest) = match &locked.source {
            Source::Registry { version, digest } => (
                Some(version.clone()),
                "registry",
                None,
                None,
                None,
                Some(digest.clone()),
            ),
            Source::Git {
                version,
                repository,
                tag,
                commit,
                digest,
            } => (
                Some(version.clone()),
                "git",
                Some(repository.clone()),
                Some(tag.clone()),
                Some(commit.clone()),
                Some(digest.clone()),
            ),
            Source::Path {
                version,
                path,
                digest,
            } => (
                Some(version.clone()),
                "path",
                Some(path.clone()),
                None,
                None,
                Some(digest.clone()),
            ),
            Source::Link { path } => (None, "link", Some(path.clone()), None, None, None),
        };
        node.detail = Detail::LockEntry {
            version,
            source: source.to_string(),
            location,
            tag,
            commit,
            digest,
        };
        out.push(node);
    }
    out
}

/// Area 8's nodes: each kind of caller `principal { }` declares, with
/// `uses` edges to its entity and the field that identifies it; each
/// sign-in method `authentication { }` declares, with the settings it
/// reads; each way `sign_in:` checks a person; and each route the runtime
/// answers itself, under what generates it, health's beside the APIs.
#[salsa::tracked(returns(ref))]
pub fn access_nodes(db: &dyn Db, project: Project) -> Part {
    let registry = registry(db, project);
    let lookup = Lookup::new(db, project);
    let identity = crate::identity::identity(db, project);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for variant in &identity.variants {
        let word = variant.kind.name();
        let Some(at) = written(db, project, "principal", &[word]) else {
            continue;
        };
        let Some(entity) = declaration_key(&lookup, &variant.entity) else {
            continue;
        };
        let key = registry.key(Kind::Principal, word, None);
        let mut node = node_at(db, project, key.clone(), Kind::Principal, &at);
        let active = ast::Member::cast(at.syntax.clone()).and_then(|member| {
            tree::members(member.block())
                .into_iter()
                .find(|(name, _)| name_text(name).as_deref() == Some("active"))
                .and_then(|(_, active)| active.value())
                .map(|value| tree::written(value.syntax()))
        });
        node.detail = Detail::Principal {
            entity: entity.clone(),
            subject: variant.subject.clone(),
            active,
        };
        uses(
            &mut edges,
            &key,
            [
                entity,
                entity_field_key(
                    registry,
                    &variant.entity,
                    std::slice::from_ref(&variant.subject),
                ),
            ],
        );
        nodes.push(node);
    }
    for method in &identity.methods {
        let word = method.method.name();
        let Some(at) = written(db, project, "authentication", &[word]) else {
            continue;
        };
        let key = registry.key(Kind::AuthenticationMethod, word, None);
        let mut node = node_at(db, project, key.clone(), Kind::AuthenticationMethod, &at);
        node.detail = Detail::AuthenticationMethod {
            built: method.built,
            cookie: method.cookie.clone(),
        };
        uses(&mut edges, &key, settings_read(db, project, &at.syntax));
        nodes.push(node);
    }
    // The ways `sign_in:` checks a person (area 8, "What is built"), each
    // sign-in provider by its connection's name (chunk A7b).
    let joined =
        |path: &[String]| -> String { path.iter().map(|segment| format!("/{segment}")).collect() };
    if let Some(passkey) = &identity.passkey
        && let Some(at) = written(db, project, "authentication", &["sign_in", "passkey"])
    {
        let key = registry.key(Kind::SignInProvider, "passkey", None);
        let mut node = node_at(db, project, key, Kind::SignInProvider, &at);
        node.detail = Detail::SignInProvider {
            path: joined(&passkey.path),
            generates_routes: identity.passkeys().is_some(),
        };
        nodes.push(node);
    }
    if !identity.providers.is_empty()
        && let Some(at) = written(db, project, "authentication", &["sign_in", "providers"])
    {
        for provider in &identity.providers {
            let key = registry.key(Kind::SignInProvider, &provider.name, None);
            let mut node = node_at(db, project, key, Kind::SignInProvider, &at);
            node.detail = Detail::SignInProvider {
                path: format!("/auth/{}", provider.name),
                generates_routes: true,
            };
            nodes.push(node);
        }
    }
    if let Some(magic) = &identity.magic_link
        && let Some(at) = written(db, project, "authentication", &["sign_in", "magic_link"])
    {
        let key = registry.key(Kind::SignInProvider, "magic_link", None);
        let mut node = node_at(db, project, key.clone(), Kind::SignInProvider, &at);
        node.detail = Detail::SignInProvider {
            path: joined(&magic.path),
            generates_routes: identity.magic_link_routes().is_some(),
        };
        // The address it sends to, and the application's own email.
        if let Some(user) = identity.variant(jadpo_std::http::PrincipalKind::User) {
            uses(
                &mut edges,
                &key,
                [entity_field_key(
                    registry,
                    &user.entity,
                    std::slice::from_ref(&magic.address),
                )],
            );
        }
        let last = |names: &Option<crate::identity::FirstSignIn>| {
            names
                .as_ref()
                .and_then(|written| written.names.last())
                .and_then(|name| declaration_key(&lookup, name))
        };
        uses(&mut edges, &key, last(&magic.email));
        // The action that sends the sign-in email itself, which the
        // runtime runs.
        if let Some(send) = last(&magic.send) {
            edges.push((key.clone(), EdgeKind::Runs, send));
        }
        nodes.push(node);
    }
    for (api, generating) in crate::routes::generated::generating(db, project) {
        for what in &generating {
            let parent = match what {
                crate::routes::generated::Generating::Exchange => {
                    registry.key(Kind::AuthenticationMethod, "service_key", None)
                }
                crate::routes::generated::Generating::Passkey(_) => {
                    registry.key(Kind::SignInProvider, "passkey", None)
                }
                crate::routes::generated::Generating::MagicLink { .. } => {
                    registry.key(Kind::SignInProvider, "magic_link", None)
                }
                crate::routes::generated::Generating::Provider(name) => {
                    registry.key(Kind::SignInProvider, name, None)
                }
                crate::routes::generated::Generating::OpenApi(_)
                | crate::routes::generated::Generating::Flags(_) => {
                    registry.key(Kind::Api, &api, None)
                }
                // Below, whether its API is declared or not.
                crate::routes::generated::Generating::Webhook(_) => continue,
            };
            for route in what.routes() {
                nodes.push(generated_route(
                    registry,
                    &route.label(),
                    Some(&api),
                    route.generated,
                    route.authentication,
                    Some(parent.clone()),
                ));
            }
        }
    }
    // Each connection's webhook, which the runtime answers whether or not
    // the app declares the `webhooks` API its record lists it in (area 11,
    // "Receiving from outside").
    for webhook in &crate::webhooks::webhooks(db, project).routes {
        let what = crate::routes::generated::Generating::Webhook(Box::new(webhook.clone()));
        for route in what.routes() {
            nodes.push(generated_route(
                registry,
                &route.label(),
                Some(&webhook.api),
                route.generated,
                route.authentication,
                Some(registry.key(Kind::Connection, &webhook.connection, None)),
            ));
        }
    }
    // Health, which the runtime answers before any route of any API (area
    // 7, "Health").
    if super::has_apis(db, project) {
        for path in [jadpo_std::http::HEALTH_LIVE, jadpo_std::http::HEALTH_READY] {
            let label = format!("GET /{}", path.join("/"));
            nodes.push(generated_route(
                registry, &label, None, "health", "none", None,
            ));
        }
    }
    (nodes, edges)
}

fn generated_route(
    registry: &jadpo_graph::Registry,
    label: &str,
    api: Option<&str>,
    generated: &str,
    authentication: &str,
    parent: Option<Key>,
) -> Node {
    let key = registry.key(Kind::GeneratedRoute, label, None);
    let mut node = node_of(
        key,
        Kind::GeneratedRoute,
        &[label, api.unwrap_or_default(), generated, authentication],
    );
    node.parent = parent;
    node.detail = Detail::GeneratedRoute {
        api: api.map(str::to_string),
        generated: generated.to_string(),
        authentication: authentication.to_string(),
    };
    node
}

/// Area 10's nodes, and the rate limits: each setting `config { }`
/// declares, with `uses` edges to a declared type it holds; the locale
/// set `locales { }` declares; and, beside the APIs, each limit every
/// route is counted against, using the connection it is counted in.
#[salsa::tracked(returns(ref))]
pub fn service_nodes(db: &dyn Db, project: Project) -> Part {
    let registry = registry(db, project);
    let lookup = Lookup::new(db, project);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for setting in &crate::config::config(db, project).settings {
        let Some(at) = written(db, project, crate::config::CONFIG, &[&setting.name]) else {
            continue;
        };
        let key = registry.key(Kind::ConfigField, &setting.name, None);
        let mut node = node_at(db, project, key.clone(), Kind::ConfigField, &at);
        let ty = type_ref(&lookup, &setting.ty);
        uses(&mut edges, &key, ty.keys().into_iter().cloned());
        node.detail = Detail::ConfigField {
            ty,
            secret: setting.secret,
            variable: setting.variable.clone(),
            default: setting.default.clone(),
        };
        nodes.push(node);
    }
    // Each flag `flags { }` declares (area 16), using the enum it holds.
    for flag in &crate::flags::flags(db, project).flags {
        let Some(at) = written(db, project, crate::flags::FLAGS, &[&flag.name]) else {
            continue;
        };
        let key = registry.key(Kind::Flag, &flag.name, None);
        let mut node = node_at(db, project, key.clone(), Kind::Flag, &at);
        let ty = type_ref(&lookup, &flag.ty);
        uses(&mut edges, &key, ty.keys().into_iter().cloned());
        node.detail = Detail::Flag {
            ty,
            default: flag
                .values
                .get(flag.default as usize)
                .cloned()
                .unwrap_or_default(),
            owner: flag.owner.clone(),
            expires: flag.expires.clone(),
        };
        nodes.push(node);
    }
    // Each monitoring threshold (area 16), using what it watches, and
    // each disk store's floor, using its connection.
    for threshold in &crate::thresholds::thresholds(db, project).thresholds {
        let Some(member) = crate::thresholds::member_of(db, project, threshold) else {
            continue;
        };
        let at = Written {
            path: threshold.path.clone(),
            syntax: member.syntax().clone(),
        };
        let key = registry.key(Kind::Threshold, &threshold.subject, None);
        let mut node = node_at(db, project, key.clone(), Kind::Threshold, &at);
        uses(&mut edges, &key, [threshold.target.key.clone()]);
        let limits = |per_request: bool| -> Vec<(String, String)> {
            threshold
                .limits
                .iter()
                .filter(|(limit, _)| limit.per_request() == per_request)
                .map(|(limit, value)| {
                    (
                        limit.word().to_string(),
                        crate::thresholds::value_text(*limit, *value),
                    )
                })
                .collect()
        };
        node.detail = Detail::Threshold {
            watches: threshold.target.watches.word().to_string(),
            per_request: limits(true),
            across_requests: limits(false),
            within: threshold
                .across()
                .then(|| crate::thresholds::within_text(threshold.within_ms)),
        };
        nodes.push(node);
    }
    for (connection, floor) in crate::thresholds::project_floors(db, project) {
        let Some(at) = written(db, project, "application", &["connections", &connection]) else {
            continue;
        };
        let key = registry.key(
            Kind::Threshold,
            &format!("{connection}.free_space_floor"),
            None,
        );
        let mut node = node_at(db, project, key.clone(), Kind::Threshold, &at);
        uses(
            &mut edges,
            &key,
            [registry.key(Kind::Connection, &connection, None)],
        );
        node.detail = Detail::Threshold {
            watches: "disk".to_string(),
            per_request: Vec::new(),
            across_requests: vec![(
                "free_space_floor".to_string(),
                crate::thresholds::percent(u64::from(floor)),
            )],
            within: None,
        };
        nodes.push(node);
    }
    let locales = crate::locales::locales(db, project);
    if let Some(plan) = &locales.plan
        && let Some(at) = declared(db, project, "locales")
    {
        let name = |ordinal: &u32| -> String {
            jadpo_std::LOCALES
                .get(*ordinal as usize)
                .map(|(name, _)| name.to_string())
                .unwrap_or_default()
        };
        let key = registry.key(Kind::LocaleSet, "application", None);
        let mut node = node_at(db, project, key, Kind::LocaleSet, &at);
        node.detail = Detail::LocaleSet {
            default: name(&plan.default),
            supported: plan.supported.iter().map(name).collect(),
            in_progress: plan.in_progress.iter().map(name).collect(),
        };
        nodes.push(node);
    }
    if super::has_apis(db, project) {
        let store = crate::rate_limits::rate_limits(db, project)
            .store
            .as_ref()
            .map(|(connection, _)| registry.key(Kind::Connection, connection, None));
        let at = written(db, project, "application", &["rate_limits"]);
        let limits: [(&str, RateLimit); 2] = [("address", ADDRESS_LIMIT), ("caller", CALLER_LIMIT)];
        for (name, limit) in limits {
            let key = registry.key(Kind::RateLimit, name, None);
            let requests = limit.requests.to_string();
            let window = limit.window_ms.to_string();
            let mut node = node_of(key.clone(), Kind::RateLimit, &[name, &requests, &window]);
            if let Some(at) = &at
                && let Some(file) =
                    crate::lookups::file_at(db, project, Name::new(db, at.path.clone()))
            {
                node.place = Some(place(&at.path, &LineIndex::new(file.text(db)), &at.syntax));
            }
            node.detail = Detail::RateLimit {
                requests: limit.requests,
                window_ms: limit.window_ms,
            };
            uses(&mut edges, &key, store.clone());
            nodes.push(node);
        }
    }
    (nodes, edges)
}
