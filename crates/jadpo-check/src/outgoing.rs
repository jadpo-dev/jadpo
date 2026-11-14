//! Outgoing webhooks to the addresses customers register (area 16,
//! "Outgoing webhooks"; chunk C1b).
//!
//! An entity's `webhooks: { events: [...] }` makes it the customer's: each
//! of its records registers addresses through the operations generated on
//! it (`register_webhook`, `webhooks`, `remove_webhook`), and each event
//! listed is delivered to the addresses of the record it names. Each
//! listed event is a `public` event of one of the app's domains, as
//! `publishing:`'s are ([`crate::published::Reader`]), holding no `File`
//! or `FileContents`, named outside by its `wire_name:` or its domain and
//! name, no two alike; and it has exactly one field typed `<Entity>.id`,
//! which names the record whose addresses receive it, so an event never
//! reaches another customer's address.
//!
//! The addresses can't be listed as connections, so every delivery leaves
//! through the runtime-owned exception
//! [`jadpo_std::outgoing::CONNECTION`], which no connection may be called.

use std::collections::BTreeMap;

use jadpo_db::{Db, ItemKind, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::Key;
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::lookups::{Lookup, NameView};
use crate::tree::{self, name_text, node_span};
use crate::types::{Ty, TypeView};

/// What an entity's `webhooks:` gives.
pub const KEYS: [&str; 1] = ["events"];

/// The entity whose records register addresses, as far as it reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebhooksEntity {
    pub entity: String,
    /// Its file.
    pub path: String,
    /// Where `webhooks:` is written.
    pub span: Span,
    /// The events its records' addresses receive, in the order listed.
    pub events: Vec<WebhookEvent>,
}

/// One event an entity's addresses receive.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WebhookEvent {
    /// The event type's identity key.
    pub key: Key,
    /// As `events:` writes it.
    pub name: String,
    /// The type it is named by outside.
    pub type_name: String,
    /// Its one field typed `<Entity>.id`: the record whose addresses
    /// receive it.
    pub holder: String,
}

/// Every entity's `webhooks:`, and the problems with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Webhooks {
    /// The one entity whose records register addresses, when one does and
    /// its section reads cleanly.
    pub entity: Option<WebhooksEntity>,
    /// Whether any entity has a `webhooks:` section, read cleanly or not.
    pub written: bool,
    /// The events each `webhooks:` lists that are the app's own public
    /// events, by name, whether or not the rest of the section reads:
    /// each is offered outside the app.
    pub listed: Vec<String>,
    pub problems: Vec<Diagnostic>,
}

impl Webhooks {
    /// The event whose key is `event`, when its records' addresses receive
    /// it.
    pub fn of_event(&self, event: &str) -> Option<&WebhookEvent> {
        self.entity
            .as_ref()?
            .events
            .iter()
            .find(|known| known.key.as_str() == event)
    }
}

const REASON: &str = "An entity's `webhooks:` lists, in `events:`, the events the addresses its records register receive: each a `public` event of one of the app's domains, since it becomes the contract of each record's receivers, with one field typed `<Entity>.id` naming the record whose addresses it goes to, so an event never reaches another record's address.";

/// Every entity's `webhooks:`, read.
#[salsa::tracked(returns(ref))]
pub fn webhooks(db: &dyn Db, project: Project) -> Webhooks {
    let mut out = Webhooks::default();
    let model = crate::data::data_model(db, project);
    let mut first: Option<String> = None;
    for entity in model.entities.iter().filter(|entity| entity.webhooks) {
        out.written = true;
        let Some((member, span, lookup, ownership)) = section(db, project, &entity.name) else {
            continue;
        };
        if let Some(first) = &first {
            out.problems.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!(
                        "`{first}` and `{}` both register addresses for outgoing webhooks",
                        entity.name
                    ),
                )
                .at(span, "decided, not built")
                .reason("Addresses registered by the records of one entity are built: the entity whose `webhooks:` lists the events they receive. Records of a second entity registering addresses of their own aren't built yet.")
                .next(format!("Keep `webhooks:` on `{first}` alone for now")),
            );
            continue;
        }
        first = Some(entity.name.clone());
        let mut reader = crate::published::Reader {
            db,
            project,
            path: &entity.path,
            lookup: &lookup,
            ownership: &ownership,
            reason: REASON,
            audit: false,
            out: &mut out.problems,
        };
        if let Some(events) = read(&mut reader, &entity.name, &member, &span, &mut out.listed) {
            out.entity = Some(WebhooksEntity {
                entity: entity.name.clone(),
                path: entity.path.clone(),
                span,
                events,
            });
        }
    }
    // The exception's name is the runtime's own.
    if crate::data::connections(db, project)
        .kind(jadpo_std::outgoing::CONNECTION)
        .is_some()
    {
        let at = out.entity.as_ref().map(|entity| entity.span.clone());
        if let Some(at) = at {
            out.problems.push(
                Diagnostic::new(
                    &rule::PUBLISHING_INVALID,
                    format!(
                        "A connection can't be called `{}`",
                        jadpo_std::outgoing::CONNECTION
                    ),
                )
                .at(at, "leaves through `outgoing_webhooks`")
                .reason("Every delivery to an address a record registers leaves through the runtime's one network exception for outgoing webhooks, which the network report, the personal-data report and their acceptances name `outgoing_webhooks`, so no connection may take the name.")
                .next("Give the connection another name"),
            );
        }
    }
    out
}

/// The `webhooks:` member of the entity called `name`, where its key is,
/// and the names its file reads in.
fn section<'db>(
    db: &'db dyn Db,
    project: Project,
    name: &str,
) -> Option<(ast::Member, Span, Lookup<'db>, jadpo_db::Ownership)> {
    let lookup = Lookup::new(db, project);
    let symbol = lookup
        .declared(name)
        .iter()
        .find(|s| s.kind == ItemKind::Entity)?
        .clone();
    let file = crate::lookups::file_at(
        db,
        project,
        crate::lookups::Name::new(db, symbol.path.clone()),
    )?;
    let source = tree::source(db, file)?;
    let Some(Decl::EntityDecl(decl)) = tree::decl_at(&source, symbol.index) else {
        return None;
    };
    let (key, member) = tree::members(decl.member_list())
        .into_iter()
        .find(|(key, member)| {
            name_text(key).as_deref() == Some(jadpo_std::outgoing::SECTION)
                && !member
                    .value()
                    .is_some_and(|value| crate::types::looks_like_type(&value))
        })?;
    let span = node_span(&symbol.path, key.syntax());
    Some((
        member,
        span,
        Lookup::for_file(db, project, file),
        symbol.ownership,
    ))
}

/// `webhooks: { events: [...] }` on `entity`, read: each event with the
/// field naming its record.
fn read(
    reader: &mut crate::published::Reader<'_, '_>,
    entity: &str,
    member: &ast::Member,
    span: &Span,
    offered: &mut Vec<String>,
) -> Option<Vec<WebhookEvent>> {
    let Some(block) = member.block() else {
        reader.invalid(
            reader.span(member),
            format!("`{entity}`'s `webhooks:` isn't a block"),
            "not a block",
            "Write `webhooks: { events: [<Event>] }`",
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
            events = reader.events(&member);
            continue;
        }
        ok = false;
        let near = nearest(&written, KEYS.iter().copied());
        reader.invalid(
            reader.span(&key),
            format!("`webhooks:` has no key `{written}:`"),
            "not a key of `webhooks:`",
            &match near {
                Some(near) => format!("Use `{near}:` instead of `{written}:`"),
                None => format!("Remove `{written}:`"),
            },
        );
    }
    let Some(events) = events else {
        if ok && !listed {
            reader.invalid(
                span.clone(),
                format!("`{entity}`'s `webhooks:` doesn't list the events its addresses receive"),
                "no `events:`",
                "Add `events: [<Event>]`, each a `public` event naming the record it goes to",
            );
        }
        return None;
    };
    if !ok {
        return None;
    }
    offered.extend(events.iter().map(|(event, _)| {
        event
            .key
            .name()
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_string()
    }));
    let mut named: BTreeMap<String, String> = BTreeMap::new();
    let mut out = Vec::new();
    for (event, at) in events {
        if let Some(first) = named.insert(event.type_name.clone(), event.name.clone()) {
            reader.invalid(
                at,
                format!(
                    "`{}` and `{first}` are both published as `{}`",
                    event.name, event.type_name
                ),
                "named already",
                &format!("Give `{}` a `wire_name:` of its own", event.name),
            );
            return None;
        }
        let holder = holder(reader, entity, &event, &at)?;
        out.push(WebhookEvent {
            key: event.key,
            name: event.name,
            type_name: event.type_name,
            holder,
        });
    }
    Some(out)
}

/// The one field of `event` typed `<entity>.id`, which names the record
/// whose addresses receive it.
fn holder(
    reader: &mut crate::published::Reader<'_, '_>,
    entity: &str,
    event: &crate::published::PublishedEvent,
    at: &Span,
) -> Option<String> {
    let fields = reader
        .lookup
        .declared(&event.name)
        .first()
        .map(|symbol| symbol.canonical.clone())
        .and_then(|canonical| reader.lookup.fields(&canonical).map(<[_]>::to_vec))
        .unwrap_or_default();
    let names = |ty: &Ty| match ty {
        Ty::Field(of, path) => of == entity && path.len() == 1 && path[0] == "id",
        _ => false,
    };
    let naming: Vec<&crate::types::FieldDef> = fields
        .iter()
        .filter(|field| names(field.declared.strip_nullable()))
        .collect();
    match naming.as_slice() {
        [one] if !matches!(one.declared, Ty::Nullable(_)) => Some(one.name.clone()),
        [one] => {
            reader.invalid(
                at.clone(),
                format!(
                    "`{}.{}` may be `none`, so it names no `{entity}` to send to",
                    event.name, one.name
                ),
                "optional",
                &format!(
                    "Make `{}` a `{entity}.id`, without `?`, naming the record whose addresses receive it",
                    one.name
                ),
            );
            None
        }
        [] => {
            reader.invalid(
                at.clone(),
                format!("`{}` names no `{entity}` to send to", event.name),
                &format!("no field typed `{entity}.id`"),
                &format!(
                    "Add a field such as `{}_id: {entity}.id` to `{}`, naming the record whose addresses receive it",
                    snake(entity),
                    event.name
                ),
            );
            None
        }
        [first, second, ..] => {
            reader.invalid(
                at.clone(),
                format!(
                    "`{}` names two `{entity}` records, `{}` and `{}`",
                    event.name, first.name, second.name
                ),
                "which one's addresses?",
                &format!(
                    "Keep one field typed `{entity}.id` in `{}`, naming the record whose addresses receive it",
                    event.name
                ),
            );
            None
        }
    }
}

/// `OrderDesk` as a field's name begins: `order_desk`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (at, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if at > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
