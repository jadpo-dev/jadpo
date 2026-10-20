//! Area 12's events and subscribers (chunk 6.1b): which actions emit,
//! where each keeps its outbox, `application { events: { store: } }`,
//! and each subscriber read into a [`SubscriberModel`] (the event it
//! listens for, the action it runs in its own domain with arguments read
//! from the event, its retry budget and the event's field naming its
//! tenant), with the problems found on the way.
//!
//! An event is kept in the outbox of the store the action that emits it
//! writes, in the same transaction as its writes (area 12, "Outbox
//! placement, settled"); an action that writes no store keeps it in the
//! store `events: { store: }` names, and with none named it is
//! `event.store_required`. A subscriber runs as the system (area 9,
//! `Access.system`), so what it reaches that only `Access.public` grants
//! is `policy.system_grant_missing` (decided 2026-10-19).
//!
//! Subscribers are read file by file ([`subscribers_in`]) and gathered
//! for the plan ([`subscribers`]); which actions emit is worked out from
//! each body's facts without positions ([`emitting`]), as the store rules
//! read them.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub mod record;

use jadpo_db::{Db, File, ItemKind, Project, item_tree, location};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::{Key, Kind};
use jadpo_syntax::ast::{self, AstNode, Decl, Expr};

use crate::lookups::{Lookup, NameView, SigView};
use crate::resolve::{Names, Want};
use crate::std_lib::StdNames;
use crate::tree::{self, node_span};
use crate::types::{Def, Fact, FieldsBase, Ty, TypeRef, TypeView, body_facts, callables_of};

/// What a subscriber holds, in the order `jadpo fmt` writes them.
pub const SUBSCRIBER_ITEMS: [&str; 3] = ["on", "run", "retry"];

/// A subscriber's default budget (area 12, "Retries and giving up"): 10
/// attempts within 24 hours.
pub const DEFAULT_RETRY: Retry = Retry {
    attempts: 10,
    within_ms: 24 * 60 * 60 * 1000,
};

/// The most attempts `retry:` may ask for, and the longest `within:`: a
/// delivery that keeps failing for longer than a month is a fault to fix,
/// not to retry.
pub const MAX_ATTEMPTS: u32 = 100;
pub const MAX_WITHIN_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// How many times a subscriber's delivery is tried, and how long after
/// its event was emitted it is given up (area 12, `retry:`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Retry {
    pub attempts: u32,
    pub within_ms: u64,
}

/// One subscriber, without positions.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubscriberModel {
    pub key: Key,
    pub name: String,
    /// Its project-relative file and the line it starts on.
    pub path: String,
    pub line: u32,
    /// The event type it listens for, by key, which the outbox records.
    pub event: Key,
    /// The action it runs, by key: one in its own domain.
    pub run: Key,
    /// Each argument of the call, a path read from the event: empty for
    /// the event itself, `["todo_id"]` for `event.todo_id`.
    pub args: Vec<Vec<String>>,
    pub retry: Retry,
    /// The event's field holding the id of the application's tenant
    /// (root `policy { tenant: }`), which the subscriber is limited to:
    /// `None` when the application has no tenant. An event of an
    /// application with a tenant that carries no single such field
    /// limits its subscribers to no tenant at all ([`Tenant`]).
    pub tenant: Tenant,
    /// The folder of the package it is in, when it is a package's own
    /// (chunk 7.6b): it hears each connection receiving the package's
    /// webhooks.
    pub package: Option<String>,
}

/// Which tenant a subscriber's delivery is limited to (area 9, "Background
/// work"; area 12, question 8): the one its event came from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Tenant {
    /// The application has no tenant.
    Untenanted,
    /// The tenant whose id the event's field holds.
    Field(String),
    /// The event carries no single field holding the tenant's id: its
    /// subscribers reach no tenant's records (area 12, "Background work and tenants").
    None,
}

/// One subscriber as read, with its problems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscriberRead {
    pub model: Option<Arc<SubscriberModel>>,
    /// Where its declaration's name is, for a problem found later.
    pub span: Span,
    /// Where each argument of its `run:` call is, in order, for a problem
    /// with what it passes (chunk 6.1c).
    pub args: Vec<Span>,
    pub problems: Vec<Diagnostic>,
}

/// The subscribers `file` declares, read.
#[salsa::tracked(returns(ref))]
pub fn subscribers_in(db: &dyn Db, project: Project, file: File) -> Vec<SubscriberRead> {
    let items = item_tree(db, file);
    if !items
        .items
        .iter()
        .any(|item| item.kind == ItemKind::Subscriber)
    {
        return Vec::new();
    }
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let path = file.path(db);
    let ownership = &location(db, project, file).ownership;
    let lookup = Lookup::within(db, project, ownership);
    let names = Names {
        path,
        ownership,
        table: &lookup,
        types: &lookup,
        std: StdNames::get(),
        library: None,
    };
    let mut out = Vec::new();
    for item in &items.items {
        if item.kind != ItemKind::Subscriber {
            continue;
        }
        let Some(Decl::SubscriberDecl(decl)) = tree::decl_at(&source, item.index) else {
            continue;
        };
        let mut reader = Reader {
            db,
            project,
            path,
            lookup: &lookup,
            names: &names,
            domain: ownership.domain().map(str::to_string),
            package: ownership.package().cloned(),
            args: Vec::new(),
            builtin: false,
            out: Vec::new(),
        };
        let model = reader.subscriber(&decl, &item.name, ownership);
        let span = decl.name().map_or_else(
            || node_span(path, decl.syntax()),
            |name| node_span(path, name.syntax()),
        );
        out.push(SubscriberRead {
            model: model.map(Arc::new),
            span,
            args: reader.args,
            problems: reader.out,
        });
    }
    out
}

/// Every subscriber that reads cleanly, in key order.
#[salsa::tracked(returns(ref))]
pub fn subscribers(db: &dyn Db, project: Project) -> Arc<Vec<Arc<SubscriberModel>>> {
    let mut out: Vec<Arc<SubscriberModel>> = project
        .files(db)
        .iter()
        .flat_map(|&file| subscribers_in(db, project, file).iter())
        .filter_map(|read| read.model.clone())
        .collect();
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out.dedup_by(|a, b| a.key == b.key);
    Arc::new(out)
}

/// The problems of the subscribers `file` declares: how each reads, then
/// what its action reaches that only `Access.public` grants (area 9).
#[salsa::tracked(returns(ref))]
pub fn subscriber_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for read in subscribers_in(db, project, file) {
        out.extend(read.problems.iter().cloned());
        let Some(model) = &read.model else { continue };
        if !crate::policy::policy_clean(db, project) {
            continue;
        }
        if let Some(found) = crate::policy::reach::public_only(db, project, model.run.name()) {
            out.push(public_only("subscriber", &model.name, &read.span, &found));
        }
    }
    out
}

/// `policy.system_grant_missing`: the action a schedule or a subscriber
/// runs (`kind`, called `name`) reaches what only `Access.public` grants,
/// which never lets the system in on purpose.
pub(crate) fn public_only(
    kind: &str,
    name: &str,
    span: &Span,
    found: &crate::policy::reach::PublicOnly,
) -> Diagnostic {
    use crate::policy::reach::PublicOnly;
    let (what, place, next) = match found {
        PublicOnly::Entity { entity, effect } => (
            format!("{} `{entity}`", effect.word()),
            format!("`{entity}`'s `{}:`", effect.word()),
            format!(
                "Add `Access.system` to `{entity}`'s `{}:`, or reach `{entity}` from a route instead",
                effect.word()
            ),
        ),
        PublicOnly::Line { written } => (
            format!("call `{written}`"),
            format!("`{written}`'s line under `actions:`"),
            format!("Add `Access.system` to `{written}`'s line under `actions:`"),
        ),
    };
    Diagnostic::new(
        &rule::POLICY_SYSTEM_GRANT_MISSING,
        format!(
            "`{name}` runs as the system, which only `Access.public` lets {what}"
        ),
    )
    .at(span.clone(), "runs as `Access.system`")
    .reason(format!(
        "A {kind} runs as the system, which an entity grants background work by naming `Access.system`; {place} names `Access.public` and not `Access.system`, and anyone at all is never how background work is let in."
    ))
    .next(next)
}

/// `application { events: { store: } }`, read (area 12).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventsRead {
    /// The store an action that writes none keeps its outbox in.
    pub store: Option<String>,
    pub problems: Vec<Diagnostic>,
}

/// The project's `events:` settings.
#[salsa::tracked(returns(ref))]
pub fn events_settings(db: &dyn Db, project: Project) -> Arc<EventsRead> {
    let mut out = EventsRead::default();
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
    let connections = crate::data::connections(db, project);
    for decl in source.decls() {
        let Decl::SingletonDecl(singleton) = decl else {
            continue;
        };
        if singleton.keyword().map(|t| t.text().to_string()).as_deref() != Some("application") {
            continue;
        }
        for (key, member) in tree::members(singleton.member_list()) {
            if tree::name_text(&key).as_deref() != Some("events") {
                continue;
            }
            let block = member.block();
            if block.is_none() {
                out.problems.push(
                    Diagnostic::new(&rule::EVENT_SETTINGS_INVALID, "`events:` holds no block")
                        .at(node_span(path, member.syntax()), "not a block")
                        .reason("`events: { store: <connection> }` names the database an action that writes none keeps its events in.")
                        .next("Write `events: { store: main }`, naming a database connection"),
                );
            }
            for (setting, entry) in tree::members(block) {
                let name = tree::name_text(&setting).unwrap_or_default();
                if name != "store" {
                    out.problems.push(
                        Diagnostic::new(
                            &rule::EVENT_SETTINGS_INVALID,
                            format!("`{name}` isn't an events setting"),
                        )
                        .at(node_span(path, setting.syntax()), "not a setting")
                        .reason("`events:` holds one setting, `store:`, the database an action that writes none keeps its events in.")
                        .next(format!("Remove `{name}:`")),
                    );
                    continue;
                }
                let written = entry
                    .value()
                    .map(|value| tree::written(value.syntax()))
                    .unwrap_or_default();
                if connections.kind(&written) == Some(crate::data::ConnectionKind::Postgres) {
                    out.store = Some(written);
                    continue;
                }
                let databases = connections.databases();
                let span = entry.value().map_or_else(
                    || node_span(path, entry.syntax()),
                    |v| node_span(path, v.syntax()),
                );
                let mut problem = Diagnostic::new(
                    &rule::EVENT_SETTINGS_INVALID,
                    format!("`store: {written}` isn't a database"),
                )
                .at(span.clone(), "not a database")
                .reason(format!(
                    "An event is kept in a database's outbox until its subscribers have it; this application's databases are {}.",
                    crate::and_list(&databases.iter().map(|d| format!("`{d}`")).collect::<Vec<_>>())
                ));
                problem = match nearest(&written, databases.iter().map(String::as_str)) {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{written}`"))
                        .edit(span, near),
                    None => problem.next("Name a database connection"),
                };
                out.problems.push(problem);
            }
        }
    }
    Arc::new(out)
}

/// What each action in `file` emits in its own body, by event key, and
/// the actions it calls: without positions.
#[salsa::tracked(returns(ref))]
fn emits_in(
    db: &dyn Db,
    project: Project,
    file: File,
) -> BTreeMap<String, (BTreeSet<Key>, BTreeSet<String>)> {
    let mut out: BTreeMap<String, (BTreeSet<Key>, BTreeSet<String>)> = BTreeMap::new();
    for callable in callables_of(db, file) {
        let facts = body_facts(db, project, callable);
        let Some(key) = facts.key_of(&[Kind::Action]) else {
            continue;
        };
        let entry = out.entry(key.name().to_string()).or_default();
        for fact in &facts.facts {
            match fact {
                Fact::Emits(event) => {
                    entry.0.insert(event.clone());
                }
                Fact::Action(callee) => {
                    entry.1.insert(callee.name().to_string());
                }
                _ => {}
            }
        }
    }
    out
}

/// The events each action emits, itself or through any action it calls,
/// by action name: only actions that emit are listed.
#[salsa::tracked(returns(ref))]
pub fn emitting(db: &dyn Db, project: Project) -> BTreeMap<String, BTreeSet<Key>> {
    let mut own: BTreeMap<String, (BTreeSet<Key>, BTreeSet<String>)> = BTreeMap::new();
    for &file in project.files(db) {
        for (name, (emits, calls)) in emits_in(db, project, file) {
            let entry = own.entry(name.clone()).or_default();
            entry.0.extend(emits.iter().cloned());
            entry.1.extend(calls.iter().cloned());
        }
    }
    let mut out = BTreeMap::new();
    for name in own.keys() {
        let mut events = BTreeSet::new();
        let mut seen = BTreeSet::from([name.clone()]);
        let mut queue = vec![name.clone()];
        while let Some(current) = queue.pop() {
            let Some((emits, calls)) = own.get(&current) else {
                continue;
            };
            events.extend(emits.iter().cloned());
            for callee in calls {
                if seen.insert(callee.clone()) {
                    queue.push(callee.clone());
                }
            }
        }
        if !events.is_empty() {
            out.insert(name.clone(), events);
        }
    }
    out
}

/// The store the action called `action` keeps the events it emits in:
/// the one it writes, itself or through what it calls, or, writing none,
/// the one `events: { store: }` names (area 12, "Outbox placement,
/// settled"). `None` when it emits nothing, or has nowhere to keep them,
/// which is `event.store_required`.
///
/// A package's action writes no store, and keeps what it emits where its
/// package's webhooks are kept: the store `events: { store: }` names, or
/// the only store that holds the app's records (chunk 7.6b).
pub fn outbox_store(db: &dyn Db, project: Project, action: &str) -> Option<String> {
    emitting(db, project).get(action)?;
    if crate::package_code::actions(db, project)
        .iter()
        .any(|own| own.canonical == action)
    {
        return crate::webhooks::store(db, project);
    }
    crate::types::written_stores(db, project)
        .get(action)
        .and_then(|stores| stores.keys().next().cloned())
        .or_else(|| events_settings(db, project).store.clone())
}

/// `event.store_required` in `file`: each action whose own body emits,
/// that writes no store and has no `events: { store: }` to keep its
/// events in, reported at its first `emit`.
#[salsa::tracked(returns(ref))]
pub fn emit_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if events_settings(db, project).store.is_some() {
        return out;
    }
    let emits = emits_in(db, project, file);
    if emits.values().all(|(own, _)| own.is_empty()) {
        return out;
    }
    // A package's action that emits is reported where it emits (chunk
    // 7.6).
    if jadpo_db::location(db, project, file)
        .ownership
        .package()
        .is_some()
    {
        return out;
    }
    let written = crate::types::written_stores(db, project);
    let path = file.path(db);
    let databases = crate::data::connections(db, project).databases();
    for callable in callables_of(db, file) {
        let checked = crate::types::typed_body(db, project, callable);
        let Some(key) = checked
            .key
            .as_ref()
            .filter(|key| key.kind() == Some(Kind::Action))
        else {
            continue;
        };
        let name = key.name();
        if written.get(name).is_some_and(|stores| !stores.is_empty()) {
            continue;
        }
        let Some((Fact::Emits(event), (start, end))) = crate::types::facts_at(&checked.body)
            .into_iter()
            .find(|(fact, _)| matches!(fact, Fact::Emits(_)))
        else {
            continue;
        };
        let store = databases.first().cloned().unwrap_or_else(|| "main".into());
        out.push(
            Diagnostic::new(
                &rule::EVENT_STORE_REQUIRED,
                format!("`{name}` emits `{}` but writes no store to keep it in", event.name().rsplit('.').next().unwrap_or_default()),
            )
            .at(Span::new(path, start, end), "no outbox")
            .reason(format!(
                "An event is kept in the outbox of the store its action writes, in the same transaction, until its subscribers have it; `{name}` writes no store, itself or through what it calls, and `app.jadpo` names none with `events: {{ store: }}`."
            ))
            .next(format!(
                "Add `events: {{ store: {store} }}` to `application {{ }}` in `app.jadpo`"
            )),
        );
    }
    out
}

/// Reads one subscriber.
struct Reader<'a> {
    db: &'a dyn Db,
    project: Project,
    path: &'a str,
    lookup: &'a Lookup<'a>,
    names: &'a Names<'a>,
    /// The subscriber's own domain, whose action it runs.
    domain: Option<String>,
    /// The package it is in, when it is a package's own (chunk 7.6b): it
    /// listens for its webhooks' events and runs one of its actions.
    package: Option<jadpo_db::PackageHome>,
    /// Where each argument of `run:` is.
    args: Vec<Span>,
    /// Whether `on:` names a built-in sign-in event (chunk 7.3b4).
    builtin: bool,
    out: Vec<Diagnostic>,
}

impl Reader<'_> {
    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    fn subscriber(
        &mut self,
        decl: &ast::SubscriberDecl,
        name: &str,
        ownership: &jadpo_db::Ownership,
    ) -> Option<SubscriberModel> {
        let mut items: BTreeMap<String, ast::Member> = BTreeMap::new();
        for item in decl
            .member_list()
            .iter()
            .flat_map(|list| list.member_items())
        {
            let ast::MemberItem::Member(member) = item else {
                self.out.push(self.item_invalid(
                    self.span(&item),
                    "This isn't one of a subscriber's items".to_string(),
                    "not an item",
                ));
                continue;
            };
            let Some(key) = member.member_key() else {
                continue;
            };
            let written = tree::written(key.syntax());
            if !SUBSCRIBER_ITEMS.contains(&written.as_str()) {
                let near = nearest(&written, SUBSCRIBER_ITEMS.iter().copied());
                let mut problem = self.item_invalid(
                    self.span(&key),
                    format!("`{written}` isn't something a subscriber holds"),
                    "not a subscriber item",
                );
                if let Some(near) = near {
                    problem = problem
                        .next(format!("Write `{near}:` instead of `{written}:`"))
                        .edit(self.span(&key), near);
                }
                self.out.push(problem);
                continue;
            }
            if items.contains_key(&written) {
                self.out.push(self.item_invalid(
                    self.span(&key),
                    format!("`{written}:` is given twice"),
                    "given again",
                ));
                continue;
            }
            items.insert(written, member);
        }
        let head = decl
            .name()
            .map_or_else(|| self.span(decl), |name| self.span(&name));
        for required in ["on", "run"] {
            if !items.contains_key(required) {
                self.out.push(self.item_invalid(
                    head.clone(),
                    format!("`{name}` has no `{required}:`"),
                    &format!("no `{required}:`"),
                ));
            }
        }
        let event = items.get("on").and_then(|member| self.event(member));
        let retry = match items.get("retry") {
            Some(member) => self.retry(member)?,
            None => DEFAULT_RETRY,
        };
        let (event_name, event_key) = event?;
        let (run, args) = self.run(items.get("run")?, &event_name)?;
        let registry = crate::graph::registry(self.db, self.project);
        let key = crate::graph::item_key(
            registry,
            Kind::Subscriber,
            &ItemKind::Subscriber,
            name,
            ownership,
        );
        let line = decl.syntax().text_range().start().into();
        let line = location_line(self.db, self.project, self.path, line);
        Some(SubscriberModel {
            key,
            name: name.to_string(),
            path: self.path.to_string(),
            line,
            tenant: self.tenant(&event_name),
            event: event_key,
            run,
            args,
            retry,
            package: self.package.as_ref().map(|home| home.folder.clone()),
        })
    }

    fn item_invalid(&self, span: Span, message: String, label: &str) -> Diagnostic {
        Diagnostic::new(&rule::SUBSCRIBER_ITEM_INVALID, message)
            .at(span, label.to_string())
            .reason("A subscriber names the event it listens for with `on:` and the action it runs with `run:`, and may change how it is retried with `retry: { attempts: within: }`; nothing else.")
            .next("Write `on: <Event>` and `run: <action>(event)`, once each")
    }

    /// `on:`: an event type, with its key.
    fn event(&mut self, member: &ast::Member) -> Option<(String, Key)> {
        let value = member.value()?;
        let mut problems = Vec::new();
        self.names.type_expr(&value, Want::Type, &mut problems);
        if !problems.is_empty() {
            self.out.extend(problems);
            return None;
        }
        let ty = self.lookup.resolve(&TypeRef::from_expr(&value));
        if ty.has_error() {
            return None;
        }
        // An event every sign-in publishes (area 8, "Events"; chunk
        // 7.3b4), which only the runtime emits, by magic links' sign-in,
        // and a passkey's own (chunk 5.5z2), by passkeys'.
        if let Ty::Opaque(name, args) = &ty
            && args.is_empty()
            && let Some(key) = jadpo_std::http::sign_in_event_key(name)
        {
            let identity = crate::identity::identity(self.db, self.project);
            let passkeys = jadpo_std::http::passkey_event(name);
            let published = if passkeys {
                identity.passkey.is_some()
            } else {
                identity.magic_link.is_some()
            };
            if !published {
                let (label, way, declare) = if passkeys {
                    (
                        "no passkeys",
                        format!(
                            "as a person {}",
                            if name == jadpo_std::http::PASSKEY_ADDED {
                                "adds a passkey"
                            } else {
                                "removes a passkey"
                            }
                        ),
                        ("passkey:", "`sign_in: { passkey: {} }`"),
                    )
                } else {
                    (
                        "no magic link",
                        "as a person signs in with a magic link".to_string(),
                        ("magic_link:", "`sign_in: { magic_link: { ... } }`"),
                    )
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::SUBSCRIBER_EVENT_INVALID,
                        format!("Nothing publishes `{name}`"),
                    )
                    .at(self.span(&value), label)
                    .reason(format!(
                        "`{name}` is published {way}, so an application whose `authentication {{ sign_in: }}` declares no `{}` never publishes it.",
                        declare.0
                    ))
                    .next(format!("Declare {}, or remove this subscriber", declare.1)),
                );
                return None;
            }
            self.builtin = true;
            return Some((name.clone(), Key::from_text(key)));
        }
        let event = match &ty {
            Ty::Named(name) if self.lookup.base(name) == Some(FieldsBase::Event) => name.clone(),
            _ => {
                self.out.push(
                    Diagnostic::new(
                        &rule::SUBSCRIBER_EVENT_INVALID,
                        format!("`on: {}` isn't an event", tree::written(value.syntax())),
                    )
                    .at(self.span(&value), "not an event")
                    .reason("A subscriber listens for one event, a type with the `Event` base that an action emits.")
                    .next("Name an event, declared as `type <Name> = Event { ... }` in a domain's `events/` folder"),
                );
                return None;
            }
        };
        let key = crate::graph::declaration_key(self.lookup, &event)?;
        // A webhook's event that needs a reply is answered by the action
        // its connection names, inside the request, and never kept for a
        // subscriber (area 11, "Webhooks that need a reply"; chunk 7.7b).
        let answered = crate::webhooks::package_events(self.db, self.project)
            .of
            .values()
            .flatten()
            .any(|webhook| webhook.key == key && webhook.reply.is_some());
        if answered {
            self.out.push(
                Diagnostic::new(
                    &rule::SUBSCRIBER_EVENT_INVALID,
                    format!(
                        "`{}` is answered in the provider's request",
                        tree::written(value.syntax())
                    ),
                )
                .at(self.span(&value), "needs a reply")
                .reason("A webhook event that names a `reply:` is answered inside the provider's request by the action its connection's `webhooks: { replies: }` names, and isn't kept in the outbox, so no subscriber receives it.")
                .next("Do the work in the action `replies:` names, or emit an event of the app's own from it for subscribers"),
            );
            return None;
        }
        // A package's subscriber listens for its own webhooks' events,
        // public or not (chunk 7.6b).
        if let Some(home) = &self.package {
            let webhook = crate::webhooks::package_events(self.db, self.project)
                .of
                .get(&home.folder)
                .is_some_and(|events| events.iter().any(|webhook| webhook.key == key));
            if !webhook {
                self.out.push(
                    Diagnostic::new(
                        &rule::SUBSCRIBER_EVENT_INVALID,
                        format!(
                            "`on: {}` isn't one of `{}`'s webhooks",
                            tree::written(value.syntax()),
                            home.versioned()
                        ),
                    )
                    .at(self.span(&value), "not a webhook's event")
                    .reason("A package's subscriber listens for an event its provider's webhooks bring, public or not, which its action enriches through its own API; the events the package's actions emit are for the app's subscribers.")
                    .next("Listen for one of the package's webhook events, declared with `wire_name:` in its `events/`, its provider declaring `webhooks:`"),
                );
                return None;
            }
        }
        Some((event, key))
    }

    /// `retry: { attempts: within: }`.
    fn retry(&mut self, member: &ast::Member) -> Option<Retry> {
        let mut retry = DEFAULT_RETRY;
        let Some(block) = member.block() else {
            self.out.push(self.retry_invalid(
                self.span(member),
                "`retry:` holds no block".to_string(),
                "not a block",
            ));
            return None;
        };
        let mut ok = true;
        for (setting, entry) in tree::members(Some(block)) {
            let name = tree::name_text(&setting).unwrap_or_default();
            let written = entry
                .value()
                .map(|value| tree::written(value.syntax()))
                .unwrap_or_default();
            let span = entry
                .value()
                .map_or_else(|| self.span(&entry), |value| self.span(&value));
            match name.as_str() {
                "attempts" => match written.parse::<u32>() {
                    Ok(attempts) if (1..=MAX_ATTEMPTS).contains(&attempts) => {
                        retry.attempts = attempts;
                    }
                    _ => {
                        self.out.push(self.retry_invalid(
                            span,
                            format!("`attempts: {written}` can't be how often it is tried"),
                            "not 1 to 100",
                        ));
                        ok = false;
                    }
                },
                "within" => {
                    let millis = jadpo_std::Dur::parse(&written).map(|dur| dur.millis);
                    match millis.and_then(|millis| u64::try_from(millis).ok()) {
                        Some(millis) if millis > 0 && millis <= MAX_WITHIN_MS => {
                            retry.within_ms = millis;
                        }
                        _ => {
                            self.out.push(self.retry_invalid(
                                span,
                                format!("`within: {written}` can't be how long it is retried"),
                                "not a duration up to 30 days",
                            ));
                            ok = false;
                        }
                    }
                }
                _ => {
                    self.out.push(self.retry_invalid(
                        self.span(&setting),
                        format!("`{name}` isn't part of `retry:`"),
                        "not a retry setting",
                    ));
                    ok = false;
                }
            }
        }
        ok.then_some(retry)
    }

    fn retry_invalid(&self, span: Span, message: String, label: &str) -> Diagnostic {
        Diagnostic::new(&rule::SUBSCRIBER_RETRY_INVALID, message)
            .at(span, label.to_string())
            .reason("`retry:` holds `attempts:`, how many times a delivery is tried, from 1 to 100 (10 by default), and `within:`, a duration up to 30 days after the event was emitted (24 hours by default).")
            .next("Write `retry: { attempts: 20  within: 3d }`, or leave `retry:` out for the default")
    }

    /// `run:`: a call of an action in the subscriber's own domain, each
    /// argument `event` or a field read through it.
    fn run(&mut self, member: &ast::Member, event: &str) -> Option<(Key, Vec<Vec<String>>)> {
        let value = member.value();
        let Some(Expr::CallExpr(call)) = value.clone() else {
            let span = value
                .as_ref()
                .map_or_else(|| self.span(member), |v| self.span(v));
            let written = value
                .as_ref()
                .map(|v| tree::written(v.syntax()))
                .unwrap_or_default();
            self.out.push(
                Diagnostic::new(
                    &rule::SUBSCRIBER_RUN_INVALID,
                    format!("`run: {written}` isn't a call"),
                )
                .at(span, "not a call")
                .reason("`run:` calls one action of the subscriber's own domain, passing the event or its fields; all the logic lives in the action.")
                .next("Write `run: <action>(event)`"),
            );
            return None;
        };
        let mut problems = Vec::new();
        self.names.callee(&call, false, &mut problems);
        if !problems.is_empty() {
            self.out.extend(problems);
            return None;
        }
        let callee = call.expr()?;
        let name = match &callee {
            Expr::NameRef(name) => tree::name_ref_text(name)?,
            Expr::FieldExpr(_) => {
                let (_, rest) = tree::dotted(&callee)?;
                tree::name_ref_text(rest.last()?)?
            }
            _ => return None,
        };
        let symbol = self.lookup.declared(&name).first()?.clone();
        let same_domain = match &self.package {
            // A package's subscriber runs one of its package's actions.
            Some(home) => symbol.ownership.package() == Some(home),
            None => symbol.ownership.domain().map(str::to_string) == self.domain,
        };
        if symbol.kind != ItemKind::Action || !same_domain {
            let what = if symbol.kind == ItemKind::Action {
                format!(
                    "an action of domain `{}`",
                    symbol.ownership.domain().unwrap_or("the root")
                )
            } else {
                symbol.kind.with_a().to_string()
            };
            let problem =
                Diagnostic::new(&rule::SUBSCRIBER_RUN_INVALID, format!("`{name}` is {what}"))
                    .at(self.span(&callee), what.clone());
            self.out.push(match &self.package {
                Some(home) => problem
                    .reason(format!("A package's subscriber runs one of `{}`'s own actions, which calls the package's API to enrich the event and emits the package's own events.", home.versioned()))
                    .next(format!("Run an action in the package's `actions/`, calling `{name}` from it")),
                None => problem
                    .reason("A subscriber lives in the domain that reacts, and runs an action of that domain; reacting to another domain's event is done by a subscriber in the reacting domain.")
                    .next(format!("Run an action of this domain, calling `{name}` from it if it is another domain's public action")),
            });
            return None;
        }
        let sig = self.lookup.signature(&name)?.clone();
        let written: Vec<ast::Arg> = call
            .arg_list()
            .iter()
            .flat_map(|list| list.args())
            .collect();
        let mut args = Vec::new();
        let mut typed = Vec::new();
        for arg in &written {
            let Some(expr) = arg.expr() else { continue };
            if arg.name().is_some() {
                self.out
                    .push(argument_invalid(self.span(&expr), "a named argument"));
                return None;
            }
            let (path, ty) = self.argument(&expr, event)?;
            self.args.push(self.span(&expr));
            args.push(path);
            typed.push((expr, ty));
        }
        if typed.len() != sig.params.len() {
            let span = call
                .arg_list()
                .map_or_else(|| self.span(&callee), |list| self.span(&list));
            let expected = sig.params.len();
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!(
                        "`{name}` takes {expected} {}, not {}",
                        if expected == 1 {
                            "argument"
                        } else {
                            "arguments"
                        },
                        typed.len()
                    ),
                )
                .at(span, format!("{} given", typed.len()))
                .reason(format!(
                    "`{name}` is declared with ({}).",
                    sig.params
                        .iter()
                        .map(|(param, ty)| format!("{param}: {ty}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .next(format!(
                    "Pass {expected} {}",
                    if expected == 1 { "value" } else { "values" }
                )),
            );
            return None;
        }
        let mut ok = true;
        for ((expr, actual), (param, expected)) in typed.iter().zip(&sig.params) {
            if !self.lookup.assignable(actual, expected) {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MISMATCH,
                        format!("`{param}` takes `{expected}`, not `{actual}`"),
                    )
                    .at(self.span(expr), format!("`{actual}`"))
                    .reason(format!(
                        "`{name}`'s parameter `{param}` is `{expected}`; a subscriber passes its event or the event's fields as they are."
                    ))
                    .next(format!("Pass a value of type `{expected}`")),
                );
                ok = false;
            }
        }
        if !ok {
            return None;
        }
        let key = crate::graph::declaration_key(self.lookup, &name)?;
        Some((key, args))
    }

    /// `event`, or a field read through it, with its type.
    fn argument(&mut self, expr: &Expr, event: &str) -> Option<(Vec<String>, Ty)> {
        let Some((first, rest)) = tree::dotted(expr) else {
            let what = match expr {
                Expr::Literal(_) => "a literal",
                Expr::CallExpr(_) => "a call",
                Expr::RecordExpr(_) => "a value built in the subscriber",
                _ => "an expression",
            };
            self.out.push(argument_invalid(self.span(expr), what));
            return None;
        };
        let root = tree::name_ref_text(&first)?;
        if root != "event" {
            self.out
                .push(argument_invalid(self.span(&first), &format!("`{root}`")));
            return None;
        }
        if self.builtin {
            return self.builtin_argument(event, &rest);
        }
        let mut ty = Ty::Named(event.to_string());
        let mut path = Vec::new();
        for name in &rest {
            let field = tree::name_ref_text(name)?;
            let host = match self.lookup.structure(&ty) {
                Ty::Named(host) if matches!(self.lookup.def(&host), Some(Def::Fields { .. })) => {
                    host
                }
                other => {
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_FIELD_ON_NON_RECORD,
                            format!("`{other}` has no fields"),
                        )
                        .at(self.span(name), "no fields")
                        .reason("Only objects have fields to read.")
                        .next("Pass the value itself"),
                    );
                    return None;
                }
            };
            let Some(def) = self
                .lookup
                .field(&host, std::slice::from_ref(&field))
                .cloned()
            else {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_UNKNOWN_FIELD,
                        format!("`{host}` has no field `{field}`"),
                    )
                    .at(self.span(name), "not declared")
                    .reason(format!(
                        "`{host}`'s fields are {}.",
                        crate::and_list(
                            &self
                                .lookup
                                .fields(&host)
                                .unwrap_or_default()
                                .iter()
                                .map(|f| format!("`{}`", f.name))
                                .collect::<Vec<_>>()
                        )
                    ))
                    .next(format!("Name a field of `{host}`")),
                );
                return None;
            };
            ty = self.lookup.field_type(&host, std::slice::from_ref(&field));
            if def.optional && def.default.is_none() {
                ty = ty.nullable();
            }
            path.push(field);
        }
        Some((path, ty))
    }

    /// `event`, or one of its fields, of the built-in sign-in event
    /// `event`, whose fields are the runtime's, `user_id` typed as the
    /// user entity's `id`.
    fn builtin_argument(
        &mut self,
        event: &str,
        rest: &[ast::NameRef],
    ) -> Option<(Vec<String>, Ty)> {
        let Some((first, more)) = rest.split_first() else {
            return Some((Vec::new(), Ty::Opaque(event.to_string(), Vec::new())));
        };
        let identity = crate::identity::identity(self.db, self.project);
        let fields = crate::types::sign_in_object_fields(identity, event).unwrap_or_default();
        let field = tree::name_ref_text(first)?;
        let Some((_, ty)) = fields.iter().find(|(name, _)| *name == field) else {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_UNKNOWN_FIELD,
                    format!("`{event}` has no field `{field}`"),
                )
                .at(self.span(first), "not one of its fields")
                .reason(format!(
                    "`{event}`'s fields are {}.",
                    crate::and_list(
                        &fields
                            .iter()
                            .map(|(name, _)| format!("`{name}`"))
                            .collect::<Vec<_>>()
                    )
                ))
                .next(format!("Name a field of `{event}`")),
            );
            return None;
        };
        if let Some(next) = more.first() {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_FIELD_ON_NON_RECORD,
                    format!("`{ty}` has no fields"),
                )
                .at(self.span(next), "no fields")
                .reason("Only objects have fields to read.")
                .next("Pass the value itself"),
            );
            return None;
        }
        Some((vec![field], ty.clone()))
    }

    /// The event's field holding the tenant's id, which a subscriber is
    /// limited to (area 9).
    fn tenant(&self, event: &str) -> Tenant {
        event_tenant(self.db, self.project, self.lookup, event)
    }
}

/// The field of the event `event` (by name) holding the tenant's id, which
/// its subscribers are limited to (area 9): what an `emit` of it must
/// prove it reached (chunk 6.6c).
pub(crate) fn event_tenant(
    db: &dyn Db,
    project: Project,
    lookup: &Lookup<'_>,
    event: &str,
) -> Tenant {
    let Some(tenant) = crate::policy::root_policy(db, project).tenant.clone() else {
        return Tenant::Untenanted;
    };
    let holding: Vec<String> = lookup
        .fields(event)
        .unwrap_or_default()
        .iter()
        .filter(|field| {
            matches!(field.declared.strip_nullable(), Ty::Field(host, path)
                if *host == tenant && path.as_slice() == ["id"])
        })
        .map(|field| field.name.clone())
        .collect();
    match holding.as_slice() {
        [one] => Tenant::Field(one.clone()),
        _ => Tenant::None,
    }
}

fn argument_invalid(span: Span, what: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::SUBSCRIBER_ARGUMENT_INVALID,
        format!("A subscriber passes its event, not {what}"),
    )
    .at(span, what.to_string())
    .reason("A subscriber's `run:` passes `event` or fields read through it, as written; the action works out anything else itself.")
    .next("Pass `event` or a field of it, and move the rest into the action")
}

/// The 1-based line `offset` is on in the file at `path`.
pub(crate) fn location_line(db: &dyn Db, project: Project, path: &str, offset: u32) -> u32 {
    project
        .files(db)
        .iter()
        .find(|file| file.path(db) == path)
        .map_or(1, |file| {
            let text = file.text(db);
            let end = (offset as usize).min(text.len());
            1 + text.as_bytes()[..end]
                .iter()
                .filter(|&&b| b == b'\n')
                .count() as u32
        })
}
