//! The public API record (areas 7 and 18): each API's committed, readable
//! record, `api/<api>.jadpo_record`, of its routes, the fields they take
//! and send, and the failure codes they can answer with, in the record
//! format (`record api 1`). `jadpo check` fails while a record disagrees
//! with the routes (`record.out_of_date`, [`crate::records`]) and `jadpo
//! api update` rewrites it, so every change to the API is a diff of one
//! file.
//!
//! An entry `api <name>` says how clients sign in to it (one
//! `authentication` fact per method, chunk 4.9) and whether its writes
//! must send an `Idempotency-Key` (`idempotency required`); a route for
//! one kind of caller says which (`caller user`).
//!
//! The record is the wire contract, so a field shows what a client sends
//! or reads (`Text { max_length: 200 }`), not the name of the type behind
//! it; objects and enums have an entry of their own. Everything is sorted
//! (routes by path, then method; types, fields and variants by born name;
//! failures by code), so reordering source changes nothing. It holds no
//! version: the OpenAPI document's version is a hash of the record
//! ([`openapi_version`]). A breaking change also needs a person's
//! acceptance (area 18), which comes after the first build.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::record::{self as format, Entry};
use jadpo_db::{Db, Project};
use jadpo_graph::{Base, Detail, Key, Kind, Section};

use super::wire::Wire;
use super::{GroupModel, RouteModel, Segment, groups, routes_clean};
use crate::graph::{declaration_key, type_ref};
use crate::lookups::Lookup;

/// The folder records live in.
pub const FOLDER: &str = "api";

/// An API's record file: `api/main.jadpo_record`.
pub fn path(api: &str) -> String {
    format!("{FOLDER}/{api}{}", format::EXTENSION)
}

/// What an API's record header says it is generated from.
pub fn from(api: &str) -> String {
    format!("the routes of API {api}")
}

/// The OpenAPI document's version for a record: the first 12 hex digits
/// of the SHA-256 of its text, computed at build and never stored, so two
/// branches changing the API never conflict on a counter.
pub fn openapi_version(text: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(format::normalise(text).as_bytes());
    digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()[..12]
        .to_string()
}

/// Each API's record, by API name, for every API that has routes. Empty
/// unless every route reads cleanly.
#[salsa::tracked(returns(ref))]
pub fn record_texts(db: &dyn Db, project: Project) -> BTreeMap<String, String> {
    if !routes_clean(db, project) {
        return BTreeMap::new();
    }
    let graph = crate::graph::graph(db, project);
    let lookup = Lookup::new(db, project);
    let groups = groups(db, project);
    let mut by_api: BTreeMap<String, Vec<(&GroupModel, &RouteModel)>> = BTreeMap::new();
    for (group, route) in groups.routes() {
        by_api
            .entry(group.api.clone())
            .or_default()
            .push((group, route));
    }
    let wire = Wire {
        graph,
        locales: crate::locales::locales(db, project).plan.as_ref(),
    };
    let apis = super::api_table(db, project);
    let identity = crate::identity::identity(db, project);
    // The passkey and magic-link routes belong to the first API that
    // accepts browser sessions (area 8), routes of its own or not.
    let passkey_api = super::served::sign_in_api(db, project);
    if let Some(api) = &passkey_api {
        by_api.entry(api.clone()).or_default();
    }
    // Each connection's webhook, in the `webhooks` API the app declares
    // (chunk 7.7).
    if !crate::webhooks::declared_in(db, project, crate::webhooks::DEFAULT_PREFIX).is_empty() {
        by_api
            .entry(crate::webhooks::DEFAULT_PREFIX.to_string())
            .or_default();
    }
    by_api
        .into_iter()
        .map(|(api, routes)| {
            // How clients sign in: switching a bearer-only API to cookies
            // changes what a browser's requests carry (area 8).
            let methods = super::apis::accepted_methods(db, project, &api);
            // The exchange the runtime answers where services sign in.
            let mut generated = Vec::new();
            if identity.exchanges()
                && methods.contains(&jadpo_std::AuthenticationMethod::ServiceKey)
            {
                generated.push(exchange_entry());
            }
            if let Some(passkey) = identity.passkeys()
                && passkey_api.as_deref() == Some(api.as_str())
            {
                generated.extend(passkey_entries(&passkey.path));
            }
            if let Some(magic) = identity.magic_link_routes()
                && passkey_api.as_deref() == Some(api.as_str())
            {
                generated.extend(magic_link_entries(
                    &magic.path,
                    identity.built(jadpo_std::AuthenticationMethod::ApiToken),
                ));
            }
            // Its own OpenAPI document (area 7, "OpenAPI").
            if let Some(served) = super::served::served_by(db, project, &api) {
                generated.push(openapi_entry(&served));
            }
            for webhook in crate::webhooks::declared_in(db, project, &api) {
                generated.push(webhook_entry(&webhook));
            }
            let mut head = Entry::new(format!("api {api}"));
            let mut names: Vec<&str> = methods.iter().map(|method| method.name()).collect();
            names.sort_unstable();
            for name in names {
                head.push(format!("authentication {name}"));
            }
            // Requiring a key changes what every write must send.
            if apis
                .get(&api)
                .is_some_and(|api| api.idempotency == jadpo_std::Idempotency::Required)
            {
                head.push("idempotency required");
            }
            let mut entries = Vec::new();
            if !head.members.is_empty() {
                entries.push(head);
            }
            entries.extend(write(&wire, &lookup, routes, generated));
            let text = format::write(&format::API, &from(&api), &entries);
            (api, text)
        })
        .collect()
}

fn method_order(route: &RouteModel) -> usize {
    jadpo_std::HttpMethod::ALL
        .iter()
        .position(|m| *m == route.method)
        .unwrap_or(0)
}

/// A route the runtime answers itself, as its API's record lists it:
/// where it sorts among the routes, its entry, and the entries of what it
/// answers with.
struct Generated {
    at: (String, usize),
    entry: Entry,
    sections: Vec<(String, Entry)>,
}

fn order_of(method: jadpo_std::HttpMethod) -> usize {
    jadpo_std::HttpMethod::ALL
        .iter()
        .position(|m| *m == method)
        .unwrap_or(0)
}

/// The generated `POST /auth/exchange` (area 8, "Ways to sign in"), as
/// its record lists it: the runtime's, for services.
fn exchange_entry() -> Generated {
    let path = format!("/{}", jadpo_std::http::EXCHANGE_PATH.join("/"));
    let token = jadpo_std::http::EXCHANGED_TOKEN;
    Generated {
        entry: Entry::new(format!("POST {path}"))
            .with("generated exchange")
            .with("caller service")
            .with(format!("output {token}"))
            .with("status 200"),
        at: (path, order_of(jadpo_std::HttpMethod::Post)),
        sections: vec![(
            token.to_string(),
            Entry::new(format!("output {token}"))
                .with("expires_at: Instant")
                .with("token: ServiceToken"),
        )],
    }
}

/// The generated `GET <prefix>/openapi.json` (area 7, "OpenAPI"), as the
/// record lists it: the runtime's, answering with the API's document.
fn openapi_entry(served: &super::served::Served) -> Generated {
    let path = served.path();
    let mut entry = Entry::new(format!("GET {path}")).with("generated openapi");
    if !served.authentication.needs_caller() {
        entry.push("authentication none");
    }
    entry.push("status 200");
    Generated {
        at: (path, order_of(jadpo_std::HttpMethod::Get)),
        entry,
        sections: Vec::new(),
    }
}

/// A connection's generated webhook (area 11, "Receiving from outside";
/// chunk 7.7), as the record lists it: the runtime's, its body the
/// provider's, checked by its signature, answered with no content.
fn webhook_entry(webhook: &crate::webhooks::WebhookModel) -> Generated {
    let path = webhook.path();
    Generated {
        entry: Entry::new(format!("POST {path}"))
            .with("generated webhook")
            .with(format!("connection {}", webhook.connection))
            .with("authentication signature")
            .with("status 204"),
        at: (path, order_of(jadpo_std::HttpMethod::Post)),
        sections: Vec::new(),
    }
}

/// The routes `passkey:` generates under `path` (area 8, "Passkeys"), as
/// the record lists them: the runtime's, each body and answer fixed.
fn passkey_entries(path: &[String]) -> Vec<Generated> {
    use jadpo_std::http::{PASSKEY_ID, PASSKEY_VIEW, PasskeyStep};
    PasskeyStep::ALL
        .into_iter()
        .map(|step| {
            let shown: String = step
                .segments(path)
                .iter()
                .map(|text| {
                    if text == PASSKEY_ID {
                        "/{passkey_id: Uuid}".to_string()
                    } else {
                        format!("/{text}")
                    }
                })
                .collect();
            let sorted: String = step
                .segments(path)
                .iter()
                .map(|text| format!("/{text}"))
                .collect();
            let mut entry =
                Entry::new(format!("{} {shown}", step.method().word())).with("generated passkey");
            if step.authentication().needs_caller() {
                entry.push("caller user");
            } else {
                entry.push("authentication none");
            }
            let input = match step {
                PasskeyStep::SignIn => Some("PasskeyAnswer"),
                PasskeyStep::Register => Some("PasskeyRegistration"),
                PasskeyStep::Rename => Some("PasskeyName"),
                _ => None,
            };
            if let Some(input) = input {
                entry.push(format!("input {input} (body)"));
            }
            if let Some(output) = step.output() {
                entry.push(format!("output {output}"));
            }
            entry.push(format!("status {}", step.status().code()));
            let mut sections = Vec::new();
            if step
                .output()
                .is_some_and(|output| output.contains(PASSKEY_VIEW))
            {
                sections.push((
                    PASSKEY_VIEW.to_string(),
                    Entry::new(format!("output {PASSKEY_VIEW}"))
                        .with("id: Uuid")
                        .with("last_used_at: Instant?")
                        .with("name: Text")
                        .with("registered_at: Instant"),
                ));
            }
            Generated {
                at: (sorted, order_of(step.method())),
                entry,
                sections,
            }
        })
        .collect()
}

/// The routes `magic_link:` generates under `path` (area 8, "Magic
/// links"), as the record lists them: the runtime's, anyone may call each,
/// each body and answer fixed. The redemption comes with `api_token:`.
fn magic_link_entries(path: &[String], api_tokens: bool) -> Vec<Generated> {
    use jadpo_std::http::{MAGIC_LINK_CONFIRMED, MAGIC_LINK_TOKEN, MagicLinkStep};
    MagicLinkStep::ALL
        .into_iter()
        .filter(|step| api_tokens || *step != MagicLinkStep::Redeem)
        .map(|step| {
            let shown: String = step
                .segments(path)
                .iter()
                .map(|text| format!("/{text}"))
                .collect();
            let mut entry = Entry::new(format!("{} {shown}", step.method().word()))
                .with("generated magic link")
                .with("authentication none")
                .with(format!("input {} (body)", step.input()));
            if let Some(output) = step.output() {
                entry.push(format!("output {output}"));
            }
            entry.push(format!("status {}", step.status().code()));
            let sections = match step {
                MagicLinkStep::Confirm => vec![(
                    MAGIC_LINK_CONFIRMED.to_string(),
                    Entry::new(format!("output {MAGIC_LINK_CONFIRMED}")).with("return_to: Text"),
                )],
                MagicLinkStep::Redeem => vec![(
                    MAGIC_LINK_TOKEN.to_string(),
                    Entry::new(format!("output {MAGIC_LINK_TOKEN}")).with("token: ApiToken"),
                )],
                MagicLinkStep::Request => Vec::new(),
            };
            Generated {
                at: (shown, order_of(step.method())),
                entry,
                sections,
            }
        })
        .collect()
}

/// What a declaration sorts by in the record: its born name without its
/// domain (`Todo` for a type renamed to `Task`; `Address` for
/// `billing.Address`), then its name now.
pub(crate) fn born_order(wire: &Wire<'_>, key: &Key) -> (String, String) {
    let born = key.name().rsplit('.').next().unwrap_or(key.name());
    (born.to_string(), wire.name(key))
}

/// A field's line: its name, the type a client sends or reads, and how it
/// is sent (`header "X-Request-Tag"`, `default 20`, `optional`).
pub(crate) fn field_line(
    wire: &Wire<'_>,
    field: &jadpo_graph::Node,
    refs: &mut BTreeSet<Key>,
) -> Option<String> {
    let Detail::Field {
        optional,
        default,
        header,
        ..
    } = &field.detail
    else {
        return None;
    };
    let name = field.name.rsplit('.').next().unwrap_or(&field.name);
    let mut line = format!(
        "{name}: {}",
        wire.render(&super::wire::field_type(field), refs)
    );
    if let Some(header) = header {
        line.push_str(&format!(" header {}", format::quote(header)));
    }
    if let Some(default) = default {
        line.push_str(&format!(
            " default {}",
            default.render(&|key| wire.name(key))
        ));
    } else if *optional {
        line.push_str(" optional");
    }
    Some(line)
}

/// The fields of `fields`, sorted by born name.
pub(crate) fn sorted_fields<'g>(
    wire: &Wire<'g>,
    mut fields: Vec<&'g jadpo_graph::Node>,
) -> Vec<&'g jadpo_graph::Node> {
    fields.sort_by_cached_key(|field| born_order(wire, &field.key));
    fields
}

fn write(
    wire: &Wire<'_>,
    lookup: &Lookup<'_>,
    mut routes: Vec<(&GroupModel, &RouteModel)>,
    mut generated: Vec<Generated>,
) -> Vec<Entry> {
    routes.sort_by_key(|(_, route)| (route.path(), method_order(route)));
    generated.sort_by(|a, b| a.at.cmp(&b.at));
    let mut refs: BTreeSet<Key> = BTreeSet::new();
    let mut failures: BTreeSet<Key> = BTreeSet::new();
    let mut out: Vec<Entry> = Vec::new();
    let mut generated_sections: Vec<(String, Entry)> = generated
        .iter()
        .flat_map(|generated| generated.sections.clone())
        .collect();
    let mut due = generated.into_iter().peekable();
    for (_, route) in &routes {
        while let Some(next) = due.next_if(|next| (route.path(), method_order(route)) > next.at) {
            out.push(next.entry);
        }
        let path: String = route
            .segments
            .iter()
            .map(|segment| match segment {
                Segment::Fixed(text) => format!("/{text}"),
                Segment::Param { name, ty } => {
                    format!(
                        "/{{{name}: {}}}",
                        wire.render(&type_ref(lookup, ty), &mut refs)
                    )
                }
            })
            .collect();
        let path = if path.is_empty() {
            "/".to_string()
        } else {
            path
        };
        let mut entry = Entry::new(format!("{} {path}", route.method.word()));
        // A route anyone may call says so: opening one is a change a
        // reviewer must see (area 8).
        if !route.authentication.needs_caller() {
            entry.push("authentication none");
        }
        // A route for one kind of caller refuses the other.
        if let Some(kind) = route.caller {
            entry.push(format!("caller {}", kind.name()));
        }
        if let Some(input) = &route.input {
            if let Some(key) = declaration_key(lookup, input) {
                refs.insert(key);
            }
            let source = if route.method.reads_query() {
                "query"
            } else {
                "body"
            };
            entry.push(format!("input {input} ({source})"));
        }
        if let Some(output) = &route.output {
            // A model's answer, streamed (area 7; chunk 5.5v6).
            if super::streams(output) {
                entry.push(format!(
                    "output {} (server-sent events)",
                    jadpo_std::ai::AI_STREAM
                ));
            } else {
                entry.push(format!(
                    "output {}",
                    wire.render(&type_ref(lookup, output), &mut refs)
                ));
            }
        }
        entry.push(format!("status {}", route.status.code()));
        // A route that waits for its work answers 202 when it hadn't
        // settled in time (area 7, "Waiting for follow-on work").
        if let Some(settle) = &route.settle {
            entry.push(format!(
                "settle within {}",
                jadpo_std::Dur {
                    millis: i128::from(settle.within_ms)
                }
            ));
        }
        let mut codes: Vec<String> = Vec::new();
        for failure in &route.fails {
            let Some(key) = declaration_key(lookup, failure) else {
                continue;
            };
            if let Some(Detail::Type {
                failure: Some(detail),
                ..
            }) = wire.node(&key).map(|node| &node.detail)
            {
                codes.push(detail.code.clone());
                failures.insert(key);
            }
        }
        codes.sort();
        codes.dedup();
        for code in codes {
            entry.push(format!("fails {code}"));
        }
        out.push(entry);
    }
    out.extend(due.map(|rest| rest.entry));
    // Failures' client fields name types too.
    let mut failure_entries: Vec<(String, Entry)> = Vec::new();
    for key in &failures {
        let Some(Detail::Type {
            failure: Some(detail),
            ..
        }) = wire.node(key).map(|node| &node.detail)
        else {
            continue;
        };
        let mut entry = Entry::new(format!("failure {}", detail.code));
        if let Some(status) = detail.status {
            entry.push(format!("status {status}"));
        }
        if let Some(message) = &detail.message {
            entry.push(format!("message {}", format::quote(message)));
        }
        for field in sorted_fields(wire, wire.fields(key)) {
            if let Detail::Field {
                ty,
                section: Some(Section::Client),
                ..
            } = &field.detail
            {
                let name = field.name.rsplit('.').next().unwrap_or(&field.name);
                entry.push(format!("{name}: {}", wire.render(ty, &mut refs)));
            }
        }
        failure_entries.push((detail.code.clone(), entry));
    }
    failure_entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut sections = type_entries(wire, refs);
    generated_sections.dedup();
    for (name, entry) in generated_sections {
        sections.insert((name.clone(), name), entry);
    }
    out.extend(sections.into_values());
    out.extend(failure_entries.into_iter().map(|(_, entry)| entry));
    out
}

/// The entry of each type `refs` names, and of each type their fields
/// name in turn, until none is new: objects, inputs, outputs and entities
/// with their fields, enums with their variants, each by born name. The
/// API record and the event record (chunk 6.1c) both end with these.
pub(crate) fn type_entries(
    wire: &Wire<'_>,
    mut refs: BTreeSet<Key>,
) -> BTreeMap<(String, String), Entry> {
    let mut done: BTreeSet<Key> = BTreeSet::new();
    let mut sections: BTreeMap<(String, String), Entry> = BTreeMap::new();
    while let Some(key) = refs.iter().find(|key| !done.contains(*key)).cloned() {
        done.insert(key.clone());
        let name = wire.name(&key);
        if let Some(issued) = super::wire::issued_of(&key) {
            sections.insert((name.clone(), name), super::wire::issued_entry(issued));
            continue;
        }
        let Some(node) = wire.node(&key) else {
            continue;
        };
        let order = born_order(wire, &key);
        match &node.detail {
            Detail::Type {
                base: Base::Enum, ..
            } => {
                let mut entry = Entry::new(format!("enum {name}"));
                let mut variants = wire.variants(&key);
                variants.sort_by(|a, b| a.0.cmp(&b.0));
                for (variant, fields) in variants {
                    entry.push(format!("variant {variant}"));
                    for field in sorted_fields(wire, fields) {
                        if let Detail::Field { .. } = &field.detail {
                            let field_name = field.name.rsplit('.').next().unwrap_or(&field.name);
                            entry.push_nested(format!(
                                "{field_name}: {}",
                                wire.render(&super::wire::field_type(field), &mut refs)
                            ));
                        }
                    }
                }
                sections.insert(order, entry);
            }
            detail => {
                let word = match detail {
                    Detail::Type {
                        base: Base::Input, ..
                    } => "input",
                    Detail::Type {
                        base: Base::Output, ..
                    } => "output",
                    _ if node.kind == Kind::Entity => "entity",
                    _ => "object",
                };
                let mut entry = Entry::new(format!("{word} {name}"));
                for field in sorted_fields(wire, wire.fields(&key)) {
                    if let Some(line) = field_line(wire, field, &mut refs) {
                        entry.push(line);
                    }
                }
                sections.insert(order, entry);
            }
        }
    }
    sections
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_openapi_version_is_the_start_of_the_records_sha256() {
        // SHA-256 of nothing is e3b0c44298fc1c14…; line endings don't count.
        assert_eq!(super::openapi_version(""), "e3b0c44298fc");
        assert_eq!(
            super::openapi_version("record api 1\r\n"),
            super::openapi_version("record api 1\n")
        );
    }
}
