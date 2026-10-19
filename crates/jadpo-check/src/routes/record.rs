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
    // The passkey routes belong to the first API that accepts browser
    // sessions (area 8, "Passkeys"), routes of its own or not.
    let passkey_api = identity.passkeys().and_then(|_| {
        apis.apis
            .iter()
            .find(|api| {
                super::apis::accepted_methods(db, project, &api.name)
                    .contains(&jadpo_std::AuthenticationMethod::BrowserSession)
            })
            .map(|api| api.name.clone())
    });
    if let Some(api) = &passkey_api {
        by_api.entry(api.clone()).or_default();
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
            let mut entry = Entry::new(format!("{} {shown}", step.method().word()))
                .with("generated passkey");
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
            if step.output().is_some_and(|output| output.contains(PASSKEY_VIEW)) {
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

/// What a declaration sorts by in the record: its born name without its
/// domain (`Todo` for a type renamed to `Task`; `Address` for
/// `billing.Address`), then its name now.
fn born_order(wire: &Wire<'_>, key: &Key) -> (String, String) {
    let born = key.name().rsplit('.').next().unwrap_or(key.name());
    (born.to_string(), wire.name(key))
}

/// A field's line: its name, the type a client sends or reads, and how it
/// is sent (`header "X-Request-Tag"`, `default 20`, `optional`).
fn field_line(wire: &Wire<'_>, field: &jadpo_graph::Node, refs: &mut BTreeSet<Key>) -> Option<String> {
    let Detail::Field {
        ty,
        optional,
        default,
        header,
        ..
    } = &field.detail
    else {
        return None;
    };
    let name = field.name.rsplit('.').next().unwrap_or(&field.name);
    let mut line = format!("{name}: {}", wire.render(ty, refs));
    if let Some(header) = header {
        line.push_str(&format!(" header {}", format::quote(header)));
    }
    if let Some(default) = default {
        line.push_str(&format!(" default {}", default.render(&|key| wire.name(key))));
    } else if *optional {
        line.push_str(" optional");
    }
    Some(line)
}

/// The fields of `fields`, sorted by born name.
fn sorted_fields<'g>(wire: &Wire<'g>, mut fields: Vec<&'g jadpo_graph::Node>) -> Vec<&'g jadpo_graph::Node> {
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
            entry.push(format!(
                "output {}",
                wire.render(&type_ref(lookup, output), &mut refs)
            ));
        }
        entry.push(format!("status {}", route.status.code()));
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
    // Each type a route, a field or a failure names, until none is new.
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
                        if let Detail::Field { ty, .. } = &field.detail {
                            let field_name = field.name.rsplit('.').next().unwrap_or(&field.name);
                            entry.push_nested(format!("{field_name}: {}", wire.render(ty, &mut refs)));
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
    generated_sections.dedup();
    for (name, entry) in generated_sections {
        sections.insert((name.clone(), name), entry);
    }
    out.extend(sections.into_values());
    out.extend(failure_entries.into_iter().map(|(_, entry)| entry));
    out
}

