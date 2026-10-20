//! Webhooks as events (area 11, "Receiving from outside"; area 12,
//! "Events from outside"; chunk 7.7).
//!
//! A provider package that declares `webhooks:` in `provider.jadpo` says
//! how its provider signs them and where each event's id, type and time
//! are in the body; its `events/` declare the events it sends, each with
//! the type the provider names it by, its `wire_name:`, and the time it
//! happened, `occurred_at: Instant`, which the runtime fills from the
//! provider's own. A connection to it that gives `webhooks: { secret: }`
//! turns on `POST <prefix>/<connection>` in the `webhooks` API, whose
//! runtime checks the signature, drops repeats by the event's id and keeps
//! each event in the outbox for the app's subscribers.
//!
//! [`webhooks`] reads every such connection into a [`WebhookModel`] for
//! the plan, with the problems found on the way: a package event without
//! its `wire_name:` or its `occurred_at`, and no store to keep the events
//! in. An app's own event names no `wire_name:` until queues are received
//! (chunk 7.7c).

use std::collections::BTreeMap;

use jadpo_db::{Db, Folder, Ownership, Place, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::Key;
use jadpo_syntax::ast::{AstNode, Decl};

use crate::lookups::Lookup;
use crate::tree::{self, name_text, node_span};
use crate::types::{Const, FieldsBase, Prim, Ty, TypeView, type_table};

/// The field every webhook event carries: when it happened, by the
/// provider's account (area 11, "Webhooks as events").
pub const OCCURRED_AT: &str = "occurred_at";

/// The `webhooks` API's prefix when the app declares none (area 7,
/// "Webhooks").
pub const DEFAULT_PREFIX: &str = "webhooks";

/// One event a provider package's webhooks carry.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WebhookEventModel {
    /// The event type's key, which its outbox row records.
    pub key: Key,
    /// Its name in the package, `PaymentSucceeded`.
    pub name: String,
    /// The type the provider names it by, `payment.succeeded`.
    pub wire_name: String,
    /// Whether the app may subscribe to it.
    pub public: bool,
}

/// One connection's webhooks, read and checked.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WebhookModel {
    pub connection: String,
    /// The package's name and version, `AcmePay.v1`.
    pub package: String,
    /// The API the route is in: `webhooks`, declared or not.
    pub api: String,
    /// The route's path, by segment.
    pub segments: Vec<String>,
    /// `check:`'s scheme after `SignatureScheme.`, and `header:`.
    pub check: String,
    pub header: Option<String>,
    /// The secret setting the signature is checked with.
    pub secret: String,
    pub event_id: String,
    pub event_type: String,
    pub occurred_at: Option<String>,
    /// The store its events and the ids already received are kept in.
    pub store: String,
    pub events: Vec<WebhookEventModel>,
}

impl WebhookModel {
    /// `/webhooks/payments`.
    pub fn path(&self) -> String {
        self.segments
            .iter()
            .map(|segment| format!("/{segment}"))
            .collect()
    }

    /// The scheme as the plan holds it.
    pub fn scheme(&self) -> Option<jadpo_std::provider::SignatureScheme> {
        jadpo_std::provider::SignatureScheme::from_name(&self.check, self.header.as_deref())
    }
}

/// Every connection's webhooks, and the problems with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Webhooks {
    pub routes: Vec<WebhookModel>,
    pub problems: Vec<Diagnostic>,
}

/// The path of the `webhooks` API, by segment: its declared prefix, or
/// `/webhooks` when the app declares none (area 7, "Webhooks").
pub fn prefix(db: &dyn Db, project: Project) -> Vec<String> {
    crate::routes::api_table(db, project)
        .get(DEFAULT_PREFIX)
        .map_or_else(
            || vec![DEFAULT_PREFIX.to_string()],
            |api| api.prefix.clone(),
        )
}

/// The webhooks listed in API `api`'s record and OpenAPI document: every
/// connection's, when the app declares the `webhooks` API (area 7,
/// "Generated routes"); none for an API it doesn't declare.
pub fn declared_in(db: &dyn Db, project: Project, api: &str) -> Vec<WebhookModel> {
    if api != DEFAULT_PREFIX || crate::routes::api_table(db, project).get(api).is_none() {
        return Vec::new();
    }
    webhooks(db, project).routes.clone()
}

/// The store webhook events are kept in: `events: { store: }`, or the
/// only store that holds the app's records. `None` when there is no such
/// store, or several and none named.
pub fn store(db: &dyn Db, project: Project) -> Option<String> {
    let mut stores: Vec<String> = crate::data::data_model(db, project)
        .entities
        .iter()
        .filter_map(|entity| entity.store.clone())
        .collect();
    stores.sort();
    stores.dedup();
    match &crate::events::events_settings(db, project).store {
        Some(named) => stores.contains(named).then(|| named.clone()),
        None => match stores.as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        },
    }
}

/// Every connection given a webhook secret, read with its package's events.
#[salsa::tracked(returns(ref))]
pub fn webhooks(db: &dyn Db, project: Project) -> Webhooks {
    let mut out = Webhooks::default();
    let events = package_events(db, project);
    out.problems.extend(events.problems.iter().cloned());
    out.problems.extend(app_wire_names(db, project));
    let store = store(db, project);
    let prefix = prefix(db, project);
    for connection in &crate::connections::package_connections(db, project).connections {
        let Some((secret, (path, start, end))) = &connection.webhook_secret else {
            continue;
        };
        let Some(reach) = &connection.reach.webhooks else {
            continue;
        };
        let (Some(check), Some(event_id), Some(event_type)) =
            (&reach.check, &reach.event_id, &reach.event_type)
        else {
            continue;
        };
        let Some(store) = store.clone() else {
            out.problems.push(store_required(
                db,
                project,
                &connection.name,
                Span::new(path.clone(), *start, *end),
            ));
            continue;
        };
        let mut segments = prefix.clone();
        segments.push(connection.name.clone());
        out.routes.push(WebhookModel {
            connection: connection.name.clone(),
            package: connection.package.clone(),
            api: DEFAULT_PREFIX.to_string(),
            segments,
            check: check.clone(),
            header: reach.header.clone(),
            secret: secret.clone(),
            event_id: event_id.clone(),
            event_type: event_type.clone(),
            occurred_at: reach.occurred_at.clone(),
            store,
            events: events
                .of
                .get(&connection.folder)
                .cloned()
                .unwrap_or_default(),
        });
    }
    out
}

/// `event.store_required` at a connection's `secret:`: its webhooks have
/// nowhere to be kept.
fn store_required(db: &dyn Db, project: Project, connection: &str, span: Span) -> Diagnostic {
    let databases = crate::data::connections(db, project).databases();
    let store = databases.first().cloned().unwrap_or_else(|| "main".into());
    Diagnostic::new(
        &rule::EVENT_STORE_REQUIRED,
        format!("`{connection}` receives webhooks but has no store to keep them in"),
    )
    .at(span, "no outbox")
    .reason("Each webhook's event is kept in the outbox, with the provider's id so a repeat is dropped, until the app's subscribers have it: in the store `events: { store: }` names, or the only store that holds the app's records.")
    .next(format!(
        "Add `events: {{ store: {store} }}` to `application {{ }}` in `app.jadpo`, naming a database that holds the app's records"
    ))
}

/// Each provider package's webhook events, by its folder, and their
/// problems.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageEvents {
    pub of: BTreeMap<String, Vec<WebhookEventModel>>,
    pub problems: Vec<Diagnostic>,
}

/// The events of every provider package that declares `webhooks:`, read
/// from its `events/`: each an `Event` with a text `wire_name:`, no two
/// alike, and `occurred_at: Instant`, which the runtime fills.
#[salsa::tracked(returns(ref))]
pub fn package_events(db: &dyn Db, project: Project) -> PackageEvents {
    let mut out = PackageEvents::default();
    let table = type_table(db, project);
    let contested = jadpo_db::contested(db, project);
    let mut seen: BTreeMap<(String, String), String> = BTreeMap::new();
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        let (Ownership::Package(home), Place::Folder(Folder::Events)) =
            (&location.ownership, &location.place)
        else {
            continue;
        };
        let webhooks = crate::connections::reach(db, project, home)
            .and_then(|reach| reach.webhooks)
            .is_some();
        if !webhooks {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let path = file.path(db).clone();
        let lookup = Lookup::within(db, project, &location.ownership);
        for decl in source.decls() {
            let Decl::TypeDecl(decl) = decl else {
                continue;
            };
            let Some(name) = decl.name().as_ref().and_then(name_text) else {
                continue;
            };
            let host = contested.canonical(&name, Some(&home.code));
            if table.base(&host) != Some(FieldsBase::Event) {
                continue;
            }
            let name_span = decl.name().map_or_else(
                || node_span(&path, decl.syntax()),
                |n| node_span(&path, n.syntax()),
            );
            let wire = tree::members(decl.member_list())
                .into_iter()
                .find(|(key, _)| name_text(key).as_deref() == Some(crate::types::WIRE_NAME));
            let wire_name = match &wire {
                None => {
                    out.problems.push(
                        Diagnostic::new(
                            &rule::PACKAGE_WIRE_NAME_INVALID,
                            format!("Webhook event `{name}` doesn't give `wire_name:`"),
                        )
                        .at(name_span.clone(), "no `wire_name:`")
                        .reason(format!(
                            "`{}` declares `webhooks:`, so each event it declares gives the type its provider names it by, `wire_name:`, which the runtime matches a webhook's event type against.",
                            home.versioned()
                        ))
                        .next("Add `wire_name:` with the provider's name for the event, such as `wire_name: \"payment.succeeded\"`"),
                    );
                    None
                }
                Some((key, member)) => match member.value().as_ref().map(Const::from_expr) {
                    Some(Const::Str(text)) if !text.trim().is_empty() => Some(text),
                    _ => {
                        out.problems.push(
                            Diagnostic::new(
                                &rule::PACKAGE_WIRE_NAME_INVALID,
                                format!("Webhook event `{name}`'s `wire_name:` isn't text"),
                            )
                            .at(node_span(&path, key.syntax()), "not text")
                            .reason("`wire_name:` gives the type the provider names the event by, as text.")
                            .next("Write the provider's name for the event as text, such as `wire_name: \"payment.succeeded\"`"),
                        );
                        None
                    }
                },
            };
            let mut clean = wire_name.is_some();
            if let Some(wire_name) = &wire_name
                && let Some(first) =
                    seen.insert((home.folder.clone(), wire_name.clone()), name.clone())
            {
                clean = false;
                let at = wire.as_ref().map_or(name_span.clone(), |(_, member)| {
                    member.value().map_or_else(
                        || node_span(&path, member.syntax()),
                        |v| node_span(&path, v.syntax()),
                    )
                });
                out.problems.push(
                    Diagnostic::new(
                        &rule::PACKAGE_WIRE_NAME_INVALID,
                        format!("`{name}` and `{first}` are both `{wire_name}`"),
                    )
                    .at(at, "named already")
                    .reason("The runtime finds a webhook's event by the type in its body, so each event a package declares has a `wire_name:` of its own.")
                    .next(format!("Give `{name}` the provider's own name for it")),
                );
            }
            let fields = table.fields(&host).unwrap_or_default();
            match fields.iter().find(|field| field.name == OCCURRED_AT) {
                Some(field)
                    if field.declared == Ty::Prim(Prim::Instant)
                        && field.wire_name.is_none()
                        && field.coding.is_none() => {}
                found => {
                    clean = false;
                    let (headline, label) = match found {
                        None => (
                            format!(
                                "Webhook event `{name}` doesn't declare `occurred_at: Instant`"
                            ),
                            "no `occurred_at`",
                        ),
                        Some(field) if field.declared != Ty::Prim(Prim::Instant) => (
                            format!("Webhook event `{name}`'s `occurred_at` isn't an `Instant`"),
                            "not an `Instant`",
                        ),
                        Some(_) => (
                            format!("Webhook event `{name}`'s `occurred_at` says how it is sent"),
                            "filled by the runtime",
                        ),
                    };
                    out.problems.push(
                        Diagnostic::new(&rule::PACKAGE_WIRE_INVALID, headline)
                            .at(name_span.clone(), label)
                            .reason("Every webhook event carries `occurred_at: Instant`, when it happened by the provider's account, which the runtime fills from the body's place `webhooks: { occurred_at: }` names, else the signature's time, else when it arrived; so the app can ignore an event older than what it stored.")
                            .next("Declare `occurred_at: Instant` in the event, with no settings"),
                    );
                }
            }
            for field in fields.iter().filter(|field| {
                matches!(
                    field.coding,
                    Some(crate::types::WrittenCoding::Functions { .. })
                )
            }) {
                clean = false;
                out.problems.push(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        format!(
                            "Webhook event `{name}`'s `{}` is read by the package's own functions",
                            field.name
                        ),
                    )
                    .at(name_span.clone(), "declared, not built")
                    .reason("A field read and written by a package's own `decode:` and `encode:` runs them where it is called, and this compiler can't run them on a webhook's event yet.")
                    .next("Use a built-in encoding, `wire:`, or the field's type as the provider sends it, for now"),
                );
            }
            let (Some(wire_name), true) = (wire_name, clean) else {
                continue;
            };
            let Some(key) = crate::graph::declaration_key(&lookup, &host) else {
                continue;
            };
            out.of
                .entry(home.folder.clone())
                .or_default()
                .push(WebhookEventModel {
                    key,
                    name,
                    wire_name,
                    public: decl.visibility().is_some(),
                });
        }
    }
    out
}

/// An app's own event giving `wire_name:`, which names it on a queue
/// (area 12, "Events from outside"): not built until chunk 7.7c.
fn app_wire_names(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        if !matches!(location.ownership, Ownership::Domain(_))
            || location.place != Place::Folder(Folder::Events)
        {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let path = file.path(db);
        for decl in source.decls() {
            let Decl::TypeDecl(decl) = decl else {
                continue;
            };
            for (key, _) in tree::members(decl.member_list()) {
                if name_text(&key).as_deref() != Some(crate::types::WIRE_NAME) {
                    continue;
                }
                out.push(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        "An app's event names its `wire_name:` for a queue, which isn't built yet",
                    )
                    .at(node_span(path, key.syntax()), "declared, not built")
                    .reason("The language names an app's own event on a queue it receives with `wire_name:`, but this compiler doesn't receive queues yet.")
                    .next("Do without `wire_name:` for now"),
                );
            }
        }
    }
    out
}
