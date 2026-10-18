//! The public API record (areas 7 and 18): each API's committed, readable
//! record, `api/<api>.api`, of its routes, the fields they take and send,
//! and the failure codes they can answer with. `jadpo check` fails while a
//! record disagrees with the routes ([`record_problems`]) and `jadpo api
//! update` rewrites it, so every change to the API is a diff of one file.
//!
//! An API says at the top how clients sign in to it (`authentication:
//! browser_session, api_token`, chunk 4.9) and whether its writes must
//! send an `Idempotency-Key` (`idempotency: required`); a route for one
//! kind of caller says which (`caller: user`).
//!
//! The record is the wire contract, so a field shows what a client sends
//! or reads (`Text { max_length: 200 }`), not the name of the type behind
//! it; objects and enums have a section of their own, by name. Its
//! `version:` rises by one each time `jadpo api update` changes it and is
//! the OpenAPI document's version. A breaking change also needs a person's
//! acceptance (area 18), which comes after the first build.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::{Base, Detail, Key, Kind, Section};

use super::wire::Wire;
use super::{GroupModel, RouteModel, Segment, groups, routes_clean};
use crate::graph::{declaration_key, type_ref};
use crate::lookups::Lookup;

/// The folder records live in.
pub const FOLDER: &str = "api";

/// An API's record file: `api/main.api`.
pub fn path(api: &str) -> String {
    format!("{FOLDER}/{api}.api")
}

/// The two lines every record starts with.
const HEADER: &str = "// Written by `jadpo api update` from the routes; never edit it by hand.\n// `jadpo check` fails while it disagrees with them.\n";

/// The record file for `body` at `version`.
pub fn file(body: &str, version: u64) -> String {
    format!("{HEADER}version: {version}\n\n{body}")
}

/// The version a record says it is at.
pub fn version(text: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix("version: "))
        .and_then(|version| version.trim().parse().ok())
}

/// A record's text after its version line: what is compared.
pub fn body(text: &str) -> &str {
    match text.find("\nversion: ") {
        Some(at) => {
            let rest = &text[at + 1..];
            let after = rest.find('\n').map_or(rest.len(), |end| end + 1);
            rest[after..].trim_start_matches('\n')
        }
        None => text,
    }
}

/// Each API's record body, by API name, for every API that has routes.
/// Empty unless every route reads cleanly.
#[salsa::tracked(returns(ref))]
pub fn record_bodies(db: &dyn Db, project: Project) -> BTreeMap<String, String> {
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
            let mut body = write(&wire, &lookup, routes, generated);
            let mut head = String::new();
            if !methods.is_empty() {
                let names: Vec<&str> = methods.iter().map(|method| method.name()).collect();
                head.push_str(&format!("authentication: {}\n", names.join(", ")));
            }
            // Requiring a key changes what every write must send.
            if apis
                .get(&api)
                .is_some_and(|api| api.idempotency == jadpo_std::Idempotency::Required)
            {
                head.push_str("idempotency: required\n");
            }
            if !head.is_empty() {
                head.push('\n');
                body.insert_str(0, &head);
            }
            (api, body)
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
/// where it sorts among the routes, its entry, and the sections of what
/// it answers with.
struct Generated {
    at: (String, usize),
    entry: String,
    sections: Vec<(String, String)>,
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
    Generated {
        entry: format!(
            "POST {path}\n    generated: exchange\n    caller: service\n    output: {}\n    status: 200\n\n",
            jadpo_std::http::EXCHANGED_TOKEN
        ),
        at: (path, order_of(jadpo_std::HttpMethod::Post)),
        sections: vec![(
            jadpo_std::http::EXCHANGED_TOKEN.to_string(),
            format!(
                "output {}\n    token: ServiceToken\n    expires_at: Instant\n",
                jadpo_std::http::EXCHANGED_TOKEN
            ),
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
            let mut entry = format!(
                "{} {shown}\n    generated: passkey\n",
                step.method().word()
            );
            if step.authentication().needs_caller() {
                entry.push_str("    caller: user\n");
            } else {
                entry.push_str("    authentication: none\n");
            }
            let input = match step {
                PasskeyStep::SignIn => Some("PasskeyAnswer"),
                PasskeyStep::Register => Some("PasskeyRegistration"),
                PasskeyStep::Rename => Some("PasskeyName"),
                _ => None,
            };
            if let Some(input) = input {
                entry.push_str(&format!("    input: {input} (body)\n"));
            }
            if let Some(output) = step.output() {
                entry.push_str(&format!("    output: {output}\n"));
            }
            entry.push_str(&format!("    status: {}\n\n", step.status().code()));
            let mut sections = Vec::new();
            if step.output().is_some_and(|output| output.contains(PASSKEY_VIEW)) {
                sections.push((
                    PASSKEY_VIEW.to_string(),
                    format!(
                        "output {PASSKEY_VIEW}\n    id: Uuid\n    name: Text\n    registered_at: Instant\n    last_used_at: Instant?\n"
                    ),
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

fn write(
    wire: &Wire<'_>,
    lookup: &Lookup<'_>,
    mut routes: Vec<(&GroupModel, &RouteModel)>,
    mut generated: Vec<Generated>,
) -> String {
    routes.sort_by_key(|(_, route)| (route.path(), method_order(route)));
    generated.sort_by(|a, b| a.at.cmp(&b.at));
    let mut refs: BTreeSet<Key> = BTreeSet::new();
    let mut failures: BTreeSet<Key> = BTreeSet::new();
    let mut out = String::new();
    let mut generated_sections: Vec<(String, String)> = generated
        .iter()
        .flat_map(|generated| generated.sections.clone())
        .collect();
    let mut due = generated.into_iter().peekable();
    for (_, route) in &routes {
        while let Some(next) = due.next_if(|next| (route.path(), method_order(route)) > next.at) {
            out.push_str(&next.entry);
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
        out.push_str(&format!("{} {path}\n", route.method.word()));
        // A route anyone may call says so: opening one is a change a
        // reviewer must see (area 8).
        if !route.authentication.needs_caller() {
            out.push_str("    authentication: none\n");
        }
        // A route for one kind of caller refuses the other.
        if let Some(kind) = route.caller {
            out.push_str(&format!("    caller: {}\n", kind.name()));
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
            out.push_str(&format!("    input: {input} ({source})\n"));
        }
        if let Some(output) = &route.output {
            out.push_str(&format!(
                "    output: {}\n",
                wire.render(&type_ref(lookup, output), &mut refs)
            ));
        }
        out.push_str(&format!("    status: {}\n", route.status.code()));
        let mut fails: Vec<(String, String)> = Vec::new();
        for failure in &route.fails {
            let Some(key) = declaration_key(lookup, failure) else {
                continue;
            };
            if let Some(Detail::Type {
                failure: Some(detail),
                ..
            }) = wire.node(&key).map(|node| &node.detail)
            {
                fails.push((
                    detail.code.clone(),
                    detail.status.map_or_else(String::new, |s| s.to_string()),
                ));
                failures.insert(key);
            }
        }
        if !fails.is_empty() {
            fails.sort();
            let shown: Vec<String> = fails
                .into_iter()
                .map(|(code, status)| format!("{code} {status}").trim_end().to_string())
                .collect();
            out.push_str(&format!("    fails: {}\n", shown.join(", ")));
        }
        out.push('\n');
    }
    for rest in due {
        out.push_str(&rest.entry);
    }
    // Failures' client fields name types too.
    let mut failure_text = String::new();
    let mut by_code: Vec<(String, Key)> = failures
        .iter()
        .filter_map(|key| match wire.node(key).map(|node| &node.detail) {
            Some(Detail::Type {
                failure: Some(detail),
                ..
            }) => Some((detail.code.clone(), key.clone())),
            _ => None,
        })
        .collect();
    by_code.sort();
    for (code, key) in by_code {
        let Some(Detail::Type {
            failure: Some(detail),
            ..
        }) = wire.node(&key).map(|node| &node.detail)
        else {
            continue;
        };
        let mut line = format!("failure {code}");
        if let Some(status) = detail.status {
            line.push_str(&format!(" {status}"));
        }
        if let Some(message) = &detail.message {
            line.push_str(&format!(" \"{message}\""));
        }
        failure_text.push_str(&line);
        failure_text.push('\n');
        for field in wire.fields(&key) {
            if let Detail::Field {
                ty,
                section: Some(Section::Client),
                ..
            } = &field.detail
            {
                let name = field.name.rsplit('.').next().unwrap_or(&field.name);
                failure_text.push_str(&format!("    {name}: {}\n", wire.render(ty, &mut refs)));
            }
        }
        failure_text.push('\n');
    }
    // Each type a route, a field or a failure names, until none is new.
    let mut done: BTreeSet<Key> = BTreeSet::new();
    let mut sections: BTreeMap<String, String> = BTreeMap::new();
    while let Some(key) = refs.iter().find(|key| !done.contains(*key)).cloned() {
        done.insert(key.clone());
        let name = wire.name(&key);
        if let Some(issued) = super::wire::issued_of(&key) {
            sections.insert(name, super::wire::issued_section(issued));
            continue;
        }
        let Some(node) = wire.node(&key) else {
            continue;
        };
        let mut text = String::new();
        match &node.detail {
            Detail::Type {
                base: Base::Enum, ..
            } => {
                text.push_str(&format!("enum {name}\n"));
                for (variant, fields) in wire.variants(&key) {
                    text.push_str(&format!("    {variant}\n"));
                    for field in fields {
                        if let Detail::Field { ty, .. } = &field.detail {
                            let field_name = field.name.rsplit('.').next().unwrap_or(&field.name);
                            text.push_str(&format!(
                                "        {field_name}: {}\n",
                                wire.render(ty, &mut refs)
                            ));
                        }
                    }
                }
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
                text.push_str(&format!("{word} {name}\n"));
                for field in wire.fields(&key) {
                    let Detail::Field {
                        ty,
                        optional,
                        default,
                        header,
                        ..
                    } = &field.detail
                    else {
                        continue;
                    };
                    let field_name = field.name.rsplit('.').next().unwrap_or(&field.name);
                    let mut line = format!("    {field_name}: {}", wire.render(ty, &mut refs));
                    if let Some(header) = header {
                        line.push_str(&format!(" header \"{header}\""));
                    }
                    if let Some(default) = default {
                        line.push_str(&format!(
                            " default {}",
                            default.render(&|key| wire.name(key))
                        ));
                    } else if *optional {
                        line.push_str(" optional");
                    }
                    text.push_str(&line);
                    text.push('\n');
                }
            }
        }
        sections.insert(name, text);
    }
    generated_sections.dedup();
    for (name, text) in generated_sections {
        sections.insert(name, text);
    }
    for text in sections.values() {
        out.push_str(text);
        out.push('\n');
    }
    out.push_str(&failure_text);
    let trimmed = out.trim_end_matches('\n');
    format!("{trimmed}\n")
}

/// `api.record_out_of_date`: each record that disagrees with the routes,
/// is missing, or records an API with no routes. Only once every route
/// reads cleanly, so a mistake in a route is reported once, where it is.
#[salsa::tracked(returns(ref))]
pub fn record_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    if project.layout(db) != jadpo_db::Layout::Project || !routes_clean(db, project) {
        return Vec::new();
    }
    let expected = record_bodies(db, project);
    let written: BTreeMap<String, Result<String, String>> = project
        .records(db)
        .iter()
        .filter(|record| record.path.starts_with("api/") && record.path.ends_with(".api"))
        .map(|record| (record.path.clone(), record.text.clone()))
        .collect();
    let mut out = Vec::new();
    for (api, body) in expected {
        let file = path(api);
        let span = crate::tree::file_span(&file);
        match written.get(&file) {
            None => out.push(
                Diagnostic::new(&rule::API_RECORD_OUT_OF_DATE, format!("`{file}` is missing"))
                    .at(span, "no record")
                    .reason(format!(
                        "Each API with routes has a committed record of its routes, fields and failure codes, so every change to API `{api}` shows as a diff of one file."
                    ))
                    .next("Run `jadpo api update`, and commit the record with the code"),
            ),
            Some(Err(error)) => out.push(
                Diagnostic::new(&rule::API_RECORD_OUT_OF_DATE, format!("`{file}` can't be read"))
                    .at(span, "unreadable")
                    .reason(format!("Reading it failed: {error}."))
                    .next("Run `jadpo api update` to write it again"),
            ),
            Some(Ok(text)) if super::record::body(text) != body => {
                let changes = changes(super::record::body(text), body);
                out.push(
                    Diagnostic::new(&rule::API_RECORD_OUT_OF_DATE, format!("`{file}` is out of date"))
                        .at(span, "disagrees with the routes")
                        .reason(format!(
                            "The record is a promise to API `{api}`'s clients, and the routes now say otherwise: {}.",
                            changes
                        ))
                        .next("Run `jadpo api update`, and commit the record with the code"),
                );
            }
            Some(Ok(_)) => {}
        }
    }
    for file in written.keys() {
        let api = file
            .trim_start_matches("api/")
            .trim_end_matches(".api")
            .to_string();
        if !expected.contains_key(&api) {
            out.push(
                Diagnostic::new(&rule::API_RECORD_OUT_OF_DATE, format!("`{file}` records no routes"))
                    .at(crate::tree::file_span(file), "nothing to record")
                    .reason(format!("No route belongs to API `{api}` any more, so its record promises routes that are gone."))
                    .next("Run `jadpo api update`, which removes it"),
            );
        }
    }
    out
}

/// What changed between two record bodies, by section (a route, a type or
/// a failure): `added GET /todos, changed output TodoView`.
fn changes(before: &str, after: &str) -> String {
    let sections = |text: &str| -> BTreeMap<String, String> {
        let mut out: BTreeMap<String, String> = BTreeMap::new();
        let mut current: Option<String> = None;
        for line in text.lines() {
            if line.is_empty() {
                continue;
            }
            if !line.starts_with(' ') {
                let head = match line.split_once(" \"") {
                    Some((head, _)) if line.starts_with("failure ") => head.to_string(),
                    _ => line.to_string(),
                };
                let head = if head.starts_with("failure ") {
                    head.split(' ').take(2).collect::<Vec<_>>().join(" ")
                } else {
                    head
                };
                current = Some(head.clone());
                out.entry(head).or_default().push_str(line);
            } else if let Some(head) = &current {
                out.entry(head.clone()).or_default().push_str(line);
            }
        }
        out
    };
    let before = sections(before);
    let after = sections(after);
    let mut said = Vec::new();
    for (head, text) in &after {
        match before.get(head) {
            None => said.push(format!("added `{head}`")),
            Some(old) if old != text => said.push(format!("changed `{head}`")),
            Some(_) => {}
        }
    }
    for head in before.keys() {
        if !after.contains_key(head) {
            said.push(format!("removed `{head}`"));
        }
    }
    if said.is_empty() {
        return "its text differs".to_string();
    }
    crate::and_list(&said)
}
