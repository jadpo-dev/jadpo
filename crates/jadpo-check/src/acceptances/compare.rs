//! What a change needs a person to accept (area 18, "Acceptances"), read
//! by comparing the records at the before side with the records today's
//! source writes: the API records and event records for `ApiChange`,
//! `access.jadpo_record` for `PolicyChange`, `network.jadpo_record` for
//! `NetworkChange`, `jadpo.lock` for `PackageChange`, and the domain
//! folders for `DomainChange` (Richard, 2026-10-30).
//!
//! Each comparison is of entries by heading, as a record's diff reads. A
//! record the before side doesn't have is compared with nothing: the
//! change that adds it is reviewed as its diff.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::record::{self as format, Entry, Kind};

/// One change that needs a person: what an acceptance lists, and where it
/// shows.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Need {
    /// As an acceptance lists it: a dotted name (`main.TodoView.priority`)
    /// or, for a route, quoted (`"main GET /books"`).
    pub subject: String,
    /// `ApiChange.output_field_removed`.
    pub change: String,
    /// The record the change shows in, for the message and its place.
    pub family: Family,
    /// The project-relative file it shows in, and the line there that
    /// starts its entry, when there is one.
    pub path: String,
    pub heading: Option<String>,
}

/// Which record a change was read from, which words its message uses.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    Api,
    Events,
    Access,
    Network,
    Lock,
    Domains,
    Decline,
    /// A domain's `monitoring.jadpo`, or `app.jadpo`'s disk floors.
    Thresholds,
}

fn need(
    out: &mut Vec<Need>,
    family: Family,
    path: &str,
    heading: Option<&str>,
    subject: String,
    change: &str,
) {
    out.push(Need {
        subject,
        change: change.to_string(),
        family,
        path: path.to_string(),
        heading: heading.map(str::to_string),
    });
}

/// A record's entries, or none when it can't be read.
pub fn entries(text: &str, kind: &Kind) -> Vec<Entry> {
    format::read(&format::normalise(text), kind).unwrap_or_default()
}

fn by_heading(entries: &[Entry]) -> BTreeMap<&str, &Entry> {
    entries
        .iter()
        .map(|entry| (entry.heading.as_str(), entry))
        .collect()
}

/// `"main GET /books"`: a route's subject, quoted since it isn't a dotted
/// name.
pub fn route_subject(api: &str, route: &str) -> String {
    format::quote(&format!("{api} {route}"))
}

// --- fields and their types -------------------------------------------------

/// A field line of a record, `name: Type default 20`, read.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FieldLine {
    name: String,
    ty: Shape,
    /// It may be left out: `optional`, or it has a `default`.
    optional: bool,
}

impl FieldLine {
    /// Whether a client must send it.
    fn required(&self) -> bool {
        !self.optional && !self.ty.nullable
    }
}

/// A type as a record writes it: its base, its rules and whether it may
/// be `none`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Shape {
    base: String,
    rules: BTreeMap<String, String>,
    nullable: bool,
}

/// How the values a type allows changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cmp {
    Same,
    /// Fewer values: what was sent may now be refused.
    Narrower,
    /// More values: what is read may now be something new.
    Wider,
    /// Neither: another type.
    Different,
}

impl Cmp {
    fn and(self, other: Cmp) -> Cmp {
        match (self, other) {
            (Cmp::Same, other) | (other, Cmp::Same) => other,
            (a, b) if a == b => a,
            _ => Cmp::Different,
        }
    }
}

/// The parts of `text` split at `sep` outside brackets and strings.
fn split_top(text: &str, sep: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut start = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
        } else {
            match c {
                b'"' => in_string = true,
                b'{' | b'<' | b'(' | b'[' => depth += 1,
                b'}' | b'>' | b')' | b']' => depth -= 1,
                _ if depth == 0 && text[i..].starts_with(sep) => {
                    out.push(text[start..i].to_string());
                    i += sep.len();
                    start = i;
                    continue;
                }
                _ => {}
            }
        }
        i += 1;
    }
    out.push(text[start..].to_string());
    out
}

fn shape(text: &str) -> Shape {
    let text = text.trim();
    let (text, nullable) = match text.strip_suffix('?') {
        Some(rest) => (rest.trim_end(), true),
        None => (text, false),
    };
    let parts = split_top(text, " {");
    let base = parts[0].trim().to_string();
    let mut rules = BTreeMap::new();
    if parts.len() > 1 {
        let inner = text[parts[0].len() + 2..]
            .trim()
            .trim_end_matches('}')
            .trim();
        for rule in split_top(inner, ", ") {
            if let Some((key, value)) = rule.split_once(": ") {
                rules.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    Shape {
        base,
        rules,
        nullable,
    }
}

fn field_line(line: &str) -> Option<FieldLine> {
    let (name, rest) = line.split_once(": ")?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return None;
    }
    let words = split_top(rest, " ");
    let marker = words
        .iter()
        .position(|word| matches!(word.as_str(), "header" | "default" | "optional"))
        .unwrap_or(words.len());
    let ty = words[..marker].join(" ");
    let optional = words[marker..]
        .iter()
        .any(|word| word == "default" || word == "optional");
    Some(FieldLine {
        name: name.to_string(),
        ty: shape(&ty),
        optional,
    })
}

/// A number a rule holds, for telling a bound raised from one lowered:
/// `200`, `0.50`, `1h`.
fn number(text: &str) -> Option<f64> {
    if let Some(duration) = jadpo_std::Dur::parse(text) {
        return Some(duration.millis as f64);
    }
    text.replace('_', "").parse().ok()
}

const LOWER_BOUNDS: [&str; 4] = ["min", "min_length", "min_items", "min_count"];
const UPPER_BOUNDS: [&str; 4] = ["max", "max_length", "max_items", "max_count"];

fn compare(before: &Shape, after: &Shape) -> Cmp {
    if before.base != after.base {
        return Cmp::Different;
    }
    let mut cmp = match (before.nullable, after.nullable) {
        (true, false) => Cmp::Narrower,
        (false, true) => Cmp::Wider,
        _ => Cmp::Same,
    };
    let keys: BTreeSet<&String> = before.rules.keys().chain(after.rules.keys()).collect();
    for key in keys {
        let step =
            match (before.rules.get(key), after.rules.get(key)) {
                (Some(was), Some(now)) if was == now => Cmp::Same,
                (Some(was), Some(now)) => {
                    match (number(was), number(now)) {
                        (Some(was), Some(now)) if LOWER_BOUNDS.contains(&key.as_str()) => {
                            if now > was { Cmp::Narrower } else { Cmp::Wider }
                        }
                        (Some(was), Some(now)) if UPPER_BOUNDS.contains(&key.as_str()) => {
                            if now < was { Cmp::Narrower } else { Cmp::Wider }
                        }
                        _ => Cmp::Different,
                    }
                }
                // A new rule refuses what it didn't; one gone allows more.
                (None, Some(_)) => Cmp::Narrower,
                (Some(_), None) => Cmp::Wider,
                (None, None) => Cmp::Same,
            };
        cmp = cmp.and(step);
    }
    cmp
}

fn fields(entry: Option<&Entry>) -> BTreeMap<String, FieldLine> {
    entry
        .map(|entry| {
            entry
                .members
                .iter()
                .filter_map(|member| field_line(&member.line))
                .map(|field| (field.name.clone(), field))
                .collect()
        })
        .unwrap_or_default()
}

fn variants(entry: &Entry) -> BTreeSet<String> {
    entry
        .members
        .iter()
        .filter_map(|member| member.line.strip_prefix("variant "))
        .map(str::to_string)
        .collect()
}

/// Where a compared pair of entries is, for the needs it gives.
struct Pair<'a> {
    family: Family,
    path: &'a str,
    /// The subject's start before and after (`main.TodoView`), which differ
    /// when a route switched to another type.
    was: String,
    now: String,
    heading: Option<&'a str>,
    /// Clients send it (an input), read it (an output), or both.
    sent: bool,
    read: bool,
}

/// What changed between two entries' fields that can break whoever sends
/// or reads them.
fn compare_fields(
    pair: &Pair<'_>,
    before: Option<&Entry>,
    after: Option<&Entry>,
    out: &mut Vec<Need>,
) {
    let was = fields(before);
    let now = fields(after);
    let mut add = |subject: String, change: &str| {
        need(
            out,
            pair.family.clone(),
            pair.path,
            pair.heading,
            subject,
            change,
        );
    };
    for name in was.keys() {
        if !now.contains_key(name) && pair.read {
            add(
                format!("{}.{name}", pair.was),
                "ApiChange.output_field_removed",
            );
        }
    }
    for (name, field) in &now {
        let subject = format!("{}.{name}", pair.now);
        let Some(old) = was.get(name) else {
            if pair.sent && field.required() {
                add(subject, "ApiChange.input_field_required");
            }
            continue;
        };
        let cmp = compare(&old.ty, &field.ty);
        let newly_required = old.optional && !field.optional && !field.ty.nullable;
        if pair.sent
            && (matches!(cmp, Cmp::Narrower)
                || (cmp == Cmp::Different && !pair.read)
                || newly_required)
        {
            add(subject.clone(), "ApiChange.rule_tightened");
        }
        if pair.read && matches!(cmp, Cmp::Wider | Cmp::Different) {
            add(subject, "ApiChange.output_field_retyped");
        }
    }
    if pair.sent
        && let (Some(before), Some(after)) = (before, after)
        && before.heading.starts_with("enum ")
    {
        let kept = variants(after);
        for variant in variants(before) {
            if !kept.contains(&variant) {
                add(
                    format!("{}.{variant}", pair.now),
                    "ApiChange.rule_tightened",
                );
            }
        }
    }
}

// --- API records ------------------------------------------------------------

const TYPE_WORDS: [&str; 5] = ["input", "output", "object", "entity", "enum"];

fn type_name(heading: &str) -> Option<&str> {
    let (word, name) = heading.split_once(' ')?;
    TYPE_WORDS.contains(&word).then_some(name)
}

fn is_route(heading: &str) -> bool {
    heading
        .split(' ')
        .next()
        .is_some_and(|word| jadpo_std::HttpMethod::ALL.iter().any(|m| m.word() == word))
}

/// The names in `text` that are entries of `types`.
fn names_in<'t>(text: &str, types: &BTreeMap<&'t str, &'t Entry>) -> Vec<&'t str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .filter_map(|word| types.get_key_value(word).map(|(name, _)| *name))
        .collect()
}

/// A record's type entries by name, and the names clients send and read.
type Directions<'t> = (
    BTreeMap<&'t str, &'t Entry>,
    BTreeSet<&'t str>,
    BTreeSet<&'t str>,
);

/// The types clients send (`sent`) and read (`read`) in an API record:
/// what its routes take and answer with, its failures' client fields, and
/// every type those name in turn.
fn directions(entries: &[Entry]) -> Directions<'_> {
    let types: BTreeMap<&str, &Entry> = entries
        .iter()
        .filter_map(|entry| type_name(&entry.heading).map(|name| (name, entry)))
        .collect();
    let mut sent: BTreeSet<&str> = BTreeSet::new();
    let mut read: BTreeSet<&str> = BTreeSet::new();
    for entry in entries {
        if is_route(&entry.heading) {
            sent.extend(names_in(&entry.heading, &types));
            for member in &entry.members {
                if let Some(input) = member.line.strip_prefix("input ") {
                    sent.extend(names_in(input, &types));
                } else if let Some(output) = member.line.strip_prefix("output ") {
                    read.extend(names_in(output, &types));
                }
            }
        } else if entry.heading.starts_with("failure ") {
            for member in &entry.members {
                if let Some(field) = field_line(&member.line) {
                    read.extend(names_in(&field.ty.base, &types));
                    read.extend(field.ty.rules.values().flat_map(|v| names_in(v, &types)));
                }
            }
        }
    }
    for set in [&mut sent, &mut read] {
        let mut queue: Vec<&str> = set.iter().copied().collect();
        while let Some(name) = queue.pop() {
            let Some(entry) = types.get(name) else {
                continue;
            };
            for member in &entry.members {
                let lines = std::iter::once(&member.line).chain(member.nested.iter());
                for line in lines {
                    for found in names_in(line.split_once(": ").map_or("", |(_, ty)| ty), &types) {
                        if set.insert(found) {
                            queue.push(found);
                        }
                    }
                }
            }
        }
    }
    (types, sent, read)
}

fn simple_type(text: &str) -> Option<&str> {
    let text = text.trim();
    (!text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.'))
    .then_some(text)
}

fn fact<'e>(entry: &'e Entry, word: &str) -> Option<&'e str> {
    entry
        .members
        .iter()
        .find_map(|member| member.line.strip_prefix(&format!("{word} ")))
}

/// What a change to API `api`'s record needs: a route removed or made
/// public, a required input field added, a rule tightened, an output field
/// removed or retyped, a failure code changed (areas 7 and 9).
pub fn api(api: &str, path: &str, before: &str, after: Option<&str>, out: &mut Vec<Need>) {
    let before = entries(before, &format::API);
    let after = after
        .map(|text| entries(text, &format::API))
        .unwrap_or_default();
    let (old_types, old_sent, old_read) = directions(&before);
    let (new_types, new_sent, new_read) = directions(&after);
    let new = by_heading(&after);
    for entry in &before {
        let heading = entry.heading.as_str();
        if is_route(heading) {
            let Some(now) = new.get(heading) else {
                need(
                    out,
                    Family::Api,
                    path,
                    None,
                    route_subject(api, heading),
                    "ApiChange.route_removed",
                );
                continue;
            };
            let public = |entry: &Entry| {
                entry
                    .members
                    .iter()
                    .any(|m| m.line == "authentication none")
            };
            if public(now) && !public(entry) {
                need(
                    out,
                    Family::Api,
                    path,
                    Some(heading),
                    route_subject(api, heading),
                    "PolicyChange.route_public",
                );
            }
            // A route that now takes or answers with another type: compare
            // what its clients sent and read with what it takes and answers.
            let input = |entry: &Entry| {
                fact(entry, "input")
                    .and_then(|i| i.split(' ').next())
                    .map(str::to_string)
            };
            let (was_in, now_in) = (input(entry), input(now));
            if was_in != now_in
                && let Some(now_in) = now_in.as_deref()
            {
                let pair = Pair {
                    family: Family::Api,
                    path,
                    was: format!("{api}.{}", was_in.as_deref().unwrap_or(now_in)),
                    now: format!("{api}.{now_in}"),
                    heading: Some(heading),
                    sent: true,
                    read: false,
                };
                let before_type = was_in
                    .as_deref()
                    .and_then(|name| old_types.get(name).copied());
                compare_fields(&pair, before_type, new_types.get(now_in).copied(), out);
            }
            let output = |entry: &Entry| fact(entry, "output").map(str::to_string);
            let (was_out, now_out) = (output(entry), output(now));
            if was_out != now_out
                && let Some(was_out) = was_out.as_deref().and_then(simple_type)
                && now_out
                    .as_deref()
                    .is_none_or(|now| simple_type(now).is_some())
            {
                let now_name = now_out.as_deref().and_then(simple_type);
                let pair = Pair {
                    family: Family::Api,
                    path,
                    was: format!("{api}.{was_out}"),
                    now: format!("{api}.{}", now_name.unwrap_or(was_out)),
                    heading: Some(heading),
                    sent: false,
                    read: true,
                };
                compare_fields(
                    &pair,
                    old_types.get(was_out).copied(),
                    now_name.and_then(|name| new_types.get(name).copied()),
                    out,
                );
            }
        } else if let Some(name) = type_name(heading) {
            let Some(now) = new_types.get(name) else {
                continue;
            };
            let pair = Pair {
                family: Family::Api,
                path,
                was: format!("{api}.{name}"),
                now: format!("{api}.{name}"),
                heading: Some(now.heading.as_str()),
                sent: old_sent.contains(name) && new_sent.contains(name),
                read: old_read.contains(name) && new_read.contains(name),
            };
            compare_fields(&pair, Some(entry), Some(now), out);
        } else if let Some(code) = heading.strip_prefix("failure ") {
            match new.get(heading) {
                None => need(
                    out,
                    Family::Api,
                    path,
                    None,
                    format!("{api}.{code}"),
                    "ApiChange.failure_changed",
                ),
                Some(now) if fact(now, "status") != fact(entry, "status") => need(
                    out,
                    Family::Api,
                    path,
                    Some(heading),
                    format!("{api}.{code}"),
                    "ApiChange.failure_changed",
                ),
                Some(_) => {}
            }
        }
    }
}

/// What a change to domain `domain`'s event record needs. Events waiting
/// in the outbox are read with the new shape, and subscribers read what
/// they carry, so an event is both sent and read (Richard, 2026-10-30): an
/// event removed, a field removed or retyped, a required field added or a
/// rule tightened.
pub fn events(domain: &str, path: &str, before: &str, after: Option<&str>, out: &mut Vec<Need>) {
    let before = entries(before, &format::EVENTS);
    let after = after
        .map(|text| entries(text, &format::EVENTS))
        .unwrap_or_default();
    let new = by_heading(&after);
    for entry in &before {
        let heading = entry.heading.as_str();
        let name = heading.split_once(' ').map_or(heading, |(_, name)| name);
        let Some(now) = new.get(heading) else {
            if heading.starts_with("event ") {
                need(
                    out,
                    Family::Events,
                    path,
                    None,
                    format!("{domain}.{name}"),
                    "ApiChange.event_removed",
                );
            }
            continue;
        };
        let pair = Pair {
            family: Family::Events,
            path,
            was: format!("{domain}.{name}"),
            now: format!("{domain}.{name}"),
            heading: Some(heading),
            sent: true,
            read: true,
        };
        compare_fields(&pair, Some(entry), Some(now), out);
    }
}

// --- who can do what --------------------------------------------------------

/// Callers by effect: `read` → `Access.public`, `StaffRole.admin`.
type Lists = BTreeMap<String, BTreeSet<String>>;

#[derive(Default)]
struct Access {
    tenant: Option<String>,
    /// Each entity's callers by effect, and `tenant <via>` or `shared`.
    entities: BTreeMap<String, (Lists, Option<String>)>,
    /// Each field row: `server_only`, and its own lists by effect.
    fields: BTreeMap<String, (bool, Lists)>,
    actions: BTreeMap<String, BTreeSet<String>>,
    queries: BTreeMap<String, BTreeSet<String>>,
    declines: BTreeSet<(String, String)>,
    sign_in: BTreeMap<String, String>,
    /// Each role's requirement lines (chunk A10), by `domain.Role.value`.
    roles: BTreeMap<String, BTreeSet<String>>,
}

const EFFECTS: [&str; 4] = ["create", "read", "update", "delete"];

fn access(text: &str) -> Access {
    let mut out = Access::default();
    for entry in entries(text, &format::ACCESS) {
        let Some((word, rest)) = entry.heading.split_once(' ') else {
            continue;
        };
        let lists = |entry: &Entry| {
            let mut lists: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for member in &entry.members {
                if let Some((effect, who)) = member.line.split_once(' ')
                    && EFFECTS.contains(&effect)
                {
                    let list = lists.entry(effect.to_string()).or_default();
                    if who != super::access::NOBODY {
                        list.insert(who.to_string());
                    }
                }
            }
            lists
        };
        match word {
            "tenant" => out.tenant = Some(rest.to_string()),
            "entity" => {
                let tenancy = entry.members.iter().find_map(|member| {
                    if member.line == "shared" {
                        Some("shared".to_string())
                    } else {
                        member.line.strip_prefix("tenant ").map(str::to_string)
                    }
                });
                out.entities
                    .insert(rest.to_string(), (lists(&entry), tenancy));
            }
            "field" => {
                let server_only = entry.members.iter().any(|m| m.line == "server_only");
                out.fields
                    .insert(rest.to_string(), (server_only, lists(&entry)));
            }
            "action" | "query" => {
                let who: BTreeSet<String> = entry
                    .members
                    .iter()
                    .filter_map(|m| m.line.strip_prefix("who ").map(str::to_string))
                    .collect();
                if word == "action" {
                    out.actions.insert(rest.to_string(), who);
                } else {
                    out.queries.insert(rest.to_string(), who);
                }
            }
            "decline" => {
                if let Some((rule, subject)) = rest.split_once(' ') {
                    out.declines.insert((rule.to_string(), subject.to_string()));
                }
            }
            "role" => {
                let lines: BTreeSet<String> =
                    entry.members.iter().map(|m| m.line.clone()).collect();
                out.roles.insert(rest.to_string(), lines);
            }
            "sign_in" => {
                for member in &entry.members {
                    if let Some((name, value)) = member.line.split_once(": ") {
                        out.sign_in
                            .insert(format!("{rest}.{name}"), value.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn added<'a>(was: Option<&'a BTreeSet<String>>, now: &'a BTreeSet<String>) -> bool {
    now.iter()
        .any(|who| was.is_none_or(|was| !was.contains(who)))
}

/// What a change to who can do what needs (area 9, "Protecting policy";
/// the widenings Richard listed, 2026-10-30): a caller added to any grant
/// list; a field's own row or `server_only` removed; tenancy loosened, or
/// a new entity shared by every tenant; a listed query's line removed or a
/// caller added to it; an `actions:` line added or removed; a security
/// rule declined; a sign-in setting loosened past its limit; a role's
/// requirement changed at all, or its line added or removed (chunk A10).
/// Narrowing, listing a query and a new entity of a tenant need no one.
pub fn access_needs(path: &str, before: &str, after: &str, out: &mut Vec<Need>) {
    let was = access(before);
    let now = access(after);
    let mut add = |subject: String, change: &str, heading: Option<String>| {
        out.push(Need {
            subject,
            change: change.to_string(),
            family: Family::Access,
            path: path.to_string(),
            heading,
        });
    };
    if was.tenant.is_some() && now.tenant.is_none() {
        add("tenant".to_string(), "PolicyChange.tenancy_loosened", None);
    }
    for (entity, (lists, tenancy)) in &now.entities {
        let heading = Some(format!("entity {entity}"));
        let Some((old_lists, old_tenancy)) = was.entities.get(entity) else {
            if now.tenant.is_some() && tenancy.as_deref() == Some("shared") {
                add(entity.clone(), "PolicyChange.shared_entity_added", heading);
            }
            continue;
        };
        for (effect, who) in lists {
            if added(old_lists.get(effect), who) {
                add(
                    format!("{entity}.{effect}"),
                    "PolicyChange.widened",
                    heading.clone(),
                );
            }
        }
        if old_tenancy.as_deref().is_some_and(|t| t != "shared")
            && tenancy.as_deref() == Some("shared")
        {
            add(entity.clone(), "PolicyChange.tenancy_loosened", heading);
        }
    }
    for (field, (server_only, lists)) in &was.fields {
        let entity = field.rsplit_once('.').map_or("", |(entity, _)| entity);
        if !now.entities.contains_key(entity) {
            continue;
        }
        let Some((now_server_only, now_lists)) = now.fields.get(field) else {
            add(field.clone(), "PolicyChange.restriction_removed", None);
            continue;
        };
        let heading = Some(format!("field {field}"));
        if *server_only && !now_server_only {
            add(
                field.clone(),
                "PolicyChange.restriction_removed",
                heading.clone(),
            );
            continue;
        }
        for (effect, who) in lists {
            match now_lists.get(effect) {
                None if !now_server_only => {
                    add(
                        format!("{field}.{effect}"),
                        "PolicyChange.widened",
                        heading.clone(),
                    );
                }
                Some(now_who) if added(Some(who), now_who) => {
                    add(
                        format!("{field}.{effect}"),
                        "PolicyChange.widened",
                        heading.clone(),
                    );
                }
                _ => {}
            }
        }
    }
    for (action, who) in &now.actions {
        let heading = Some(format!("action {action}"));
        match was.actions.get(action) {
            None => add(action.clone(), "PolicyChange.listing_added", heading),
            Some(old) if added(Some(old), who) => {
                add(action.clone(), "PolicyChange.widened", heading)
            }
            Some(_) => {}
        }
    }
    for action in was.actions.keys() {
        if !now.actions.contains_key(action) {
            add(action.clone(), "PolicyChange.listing_removed", None);
        }
    }
    for (query, who) in &was.queries {
        match now.queries.get(query) {
            None => add(query.clone(), "PolicyChange.listing_removed", None),
            Some(now_who) if added(Some(who), now_who) => {
                add(
                    query.clone(),
                    "PolicyChange.widened",
                    Some(format!("query {query}")),
                );
            }
            Some(_) => {}
        }
    }
    // A role's requirement changed either way, or a role's line added or
    // removed, needs a person (Richard, 2026-10-31): who keeps a role
    // through which sign-in is as much policy as who holds it.
    let named: BTreeSet<&String> = was.roles.keys().chain(now.roles.keys()).collect();
    for role in named {
        if was.roles.get(role) != now.roles.get(role) {
            add(
                role.clone(),
                "PolicyChange.role_requirements_changed",
                now.roles.contains_key(role).then(|| format!("role {role}")),
            );
        }
    }
    for (rule, subject) in &now.declines {
        if !was.declines.contains(&(rule.clone(), subject.clone())) {
            let name = rule.split_once('.').map_or(rule.as_str(), |(_, name)| name);
            out.push(Need {
                subject: subject.clone(),
                change: format!("PolicyChange.{name}_declined"),
                family: Family::Decline,
                path: path.to_string(),
                heading: Some(format!("decline {rule} {subject}")),
            });
        }
    }
    for (setting, limit) in super::access::sign_in_limits() {
        let key = format!("magic_link.{setting}");
        let Some(value) = now.sign_in.get(&key) else {
            continue;
        };
        let (Some(value), Some(limit)) = (number(value), number(&limit)) else {
            continue;
        };
        let was_value = was.sign_in.get(&key).and_then(|text| number(text));
        if value > limit && was_value.is_none_or(|was| value > was) {
            out.push(Need {
                subject: key,
                change: "PolicyChange.sign_in_loosened".to_string(),
                family: Family::Access,
                path: path.to_string(),
                heading: Some("sign_in magic_link".to_string()),
            });
        }
    }
}

// --- outside hosts, packages and domains ------------------------------------

/// What a change to the outside hosts needs: every host a connection
/// reaches that it didn't, a new connection's included (Richard,
/// 2026-10-30), named by its connection.
pub fn network(path: &str, before: &str, after: &str, out: &mut Vec<Need>) {
    let hosts = |text: &str| -> BTreeMap<String, BTreeSet<String>> {
        entries(text, &format::NETWORK)
            .into_iter()
            .filter_map(|entry| {
                let connection = entry.heading.strip_prefix("connection ")?.to_string();
                let hosts = entry
                    .members
                    .iter()
                    .filter_map(|m| m.line.strip_prefix("host ").map(str::to_string))
                    .collect();
                Some((connection, hosts))
            })
            .collect()
    };
    let was = hosts(before);
    for (connection, now) in hosts(after) {
        if added(was.get(&connection), &now) {
            need(
                out,
                Family::Network,
                path,
                Some(&format!("connection {connection}")),
                connection.clone(),
                "NetworkChange.host_added",
            );
        }
    }
}

/// A third party's package installed from git (area 11, "Publisher
/// trust"): every git package not locked from git before, since telling
/// the app's own company's repository from a third party's needs a
/// setting the pages don't decide.
pub fn lock(path: &str, before: &str, after: &str, out: &mut Vec<Need>) {
    let from_git = |text: &str| -> BTreeSet<String> {
        jadpo_db::parse_lock(&format::normalise(text))
            .unwrap_or_default()
            .into_iter()
            .filter(|locked| matches!(locked.source, jadpo_db::Source::Git { .. }))
            .map(|locked| locked.package)
            .collect()
    };
    let was = from_git(before);
    for package in from_git(after) {
        if !was.contains(&package) {
            out.push(Need {
                heading: Some(format!("package {package}")),
                subject: package,
                change: "PackageChange.git_source".to_string(),
                family: Family::Lock,
                path: path.to_string(),
            });
        }
    }
}

/// A domain created (area 1, "Domains"): its folder wasn't there before.
pub fn domains(before: &BTreeSet<String>, after: &BTreeSet<String>, out: &mut Vec<Need>) {
    for domain in after.difference(before) {
        need(
            out,
            Family::Domains,
            &format!("domains/{domain}"),
            None,
            domain.clone(),
            "DomainChange.created",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(kind: &Kind, from: &str, body: &str) -> String {
        format!("{}{body}", format::header(kind, from))
    }

    fn api_needs(before: &str, after: &str) -> Vec<(String, String)> {
        let from = "the routes of API main";
        let mut out = Vec::new();
        api(
            "main",
            "api/main.jadpo_record",
            &record(&format::API, from, before),
            Some(&record(&format::API, from, after)),
            &mut out,
        );
        out.into_iter().map(|n| (n.subject, n.change)).collect()
    }

    const TODOS: &str = "
GET /todos/{todo_id: Uuid}
    output TodoView
    status 200

POST /todos
    input CreateTodo (body)
    output TodoView
    status 201
    fails todo_locked

input CreateTodo
    title: Text { min_length: 1, max_length: 200 }

output TodoView
    id: Uuid
    priority: Int
    title: Text { min_length: 1, max_length: 200 }

failure todo_locked
    status 409
";

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn removing_an_output_field_needs_a_person() {
        let after = TODOS.replace("    priority: Int\n", "");
        assert_eq!(
            api_needs(TODOS, &after),
            pairs(&[("main.TodoView.priority", "ApiChange.output_field_removed")])
        );
    }

    #[test]
    fn adding_only_needs_no_one() {
        let after = TODOS
            .replace("    priority: Int\n", "    priority: Int\n    tags: Text?\n")
            .replace("    title: Text { min_length: 1, max_length: 200 }\n\noutput", "    note: Text optional\n    title: Text { min_length: 1, max_length: 300 }\n\noutput");
        assert_eq!(api_needs(TODOS, &after), Vec::<(String, String)>::new());
    }

    #[test]
    fn breaking_changes_are_each_named() {
        let after = TODOS
            .replace("GET /todos/{todo_id: Uuid}\n    output TodoView\n    status 200\n\n", "")
            .replace("input CreateTodo\n    title: Text { min_length: 1, max_length: 200 }", "input CreateTodo\n    due: Date\n    title: Text { min_length: 3, max_length: 200 }")
            .replace("    priority: Int\n", "    priority: Text\n")
            .replace("    status 409\n", "    status 423\n");
        assert_eq!(
            api_needs(TODOS, &after),
            pairs(&[
                (
                    "\"main GET /todos/{todo_id: Uuid}\"",
                    "ApiChange.route_removed"
                ),
                ("main.CreateTodo.due", "ApiChange.input_field_required"),
                ("main.CreateTodo.title", "ApiChange.rule_tightened"),
                ("main.TodoView.priority", "ApiChange.output_field_retyped"),
                ("main.todo_locked", "ApiChange.failure_changed"),
            ])
        );
    }

    #[test]
    fn a_route_made_public_is_a_policy_change() {
        let after = TODOS.replace("POST /todos\n", "POST /todos\n    authentication none\n");
        assert_eq!(
            api_needs(TODOS, &after),
            pairs(&[("\"main POST /todos\"", "PolicyChange.route_public")])
        );
    }

    #[test]
    fn types_are_compared_by_what_they_allow() {
        let cmp = |a: &str, b: &str| compare(&shape(a), &shape(b));
        assert_eq!(
            cmp("Text { max_length: 200 }", "Text { max_length: 100 }"),
            Cmp::Narrower
        );
        assert_eq!(cmp("Text { max_length: 200 }", "Text"), Cmp::Wider);
        assert_eq!(cmp("Int", "Int { min: 0 }"), Cmp::Narrower);
        assert_eq!(cmp("Uuid?", "Uuid"), Cmp::Narrower);
        assert_eq!(cmp("Uuid", "Text"), Cmp::Different);
        assert_eq!(
            cmp(
                "Text { min_length: 1, max_length: 200 }",
                "Text { min_length: 2, max_length: 300 }"
            ),
            Cmp::Different
        );
        assert_eq!(
            cmp("Duration { max: 1h }", "Duration { max: 2h }"),
            Cmp::Wider
        );
    }

    fn access_record(body: &str) -> String {
        record(&format::ACCESS, format::ACCESS_FROM, body)
    }

    fn widenings(before: &str, after: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        access_needs(
            "access.jadpo_record",
            &access_record(before),
            &access_record(after),
            &mut out,
        );
        out.into_iter().map(|n| (n.subject, n.change)).collect()
    }

    const LIBRARY: &str = "
tenant Branch

entity library.Book
    create StaffRole.librarian
    read Access.authenticated
    tenant branch_id

field library.Book.cost
    read StaffRole.librarian

action lending.borrow_book
    who Access.authenticated

query catalogue.staff_books
    who StaffRole.librarian

sign_in magic_link
    code_attempts: 5
    expires_after: 15m
    failed_codes_per_day: 20
";

    #[test]
    fn every_widening_needs_a_person() {
        let after = LIBRARY
            .replace("    read Access.authenticated\n", "    read Access.authenticated\n    read Access.public\n")
            .replace("    tenant branch_id\n", "    shared\n")
            .replace("field library.Book.cost\n    read StaffRole.librarian\n\n", "")
            .replace("query catalogue.staff_books\n    who StaffRole.librarian\n\n", "")
            .replace("    expires_after: 15m\n", "    expires_after: 2h\n")
            .replace("\nsign_in", "\naction lending.renew_loan\n    who Access.authenticated\n\ndecline policy.call_ungranted lending.borrow_book\n\nsign_in");
        assert_eq!(
            widenings(LIBRARY, &after),
            pairs(&[
                ("library.Book.read", "PolicyChange.widened"),
                ("library.Book", "PolicyChange.tenancy_loosened"),
                ("library.Book.cost", "PolicyChange.restriction_removed"),
                ("lending.renew_loan", "PolicyChange.listing_added"),
                ("catalogue.staff_books", "PolicyChange.listing_removed"),
                (
                    "lending.borrow_book",
                    "PolicyChange.call_ungranted_declined"
                ),
                ("magic_link.expires_after", "PolicyChange.sign_in_loosened"),
            ])
        );
    }

    #[test]
    fn narrowing_needs_no_one() {
        let after = LIBRARY
            .replace("    create StaffRole.librarian\n", "")
            .replace("    expires_after: 15m\n", "    expires_after: 1h\n")
            .replace("\nsign_in", "\nquery catalogue.new_books\n    who Access.authenticated\n\nsign_in")
            .replace("\nfield library.Book.cost", "\nentity library.Shelf\n    read Access.authenticated\n    tenant branch_id\n\nfield library.Book.cost");
        assert_eq!(widenings(LIBRARY, &after), Vec::<(String, String)>::new());
    }

    /// Any change to how a role must be signed in to needs a person,
    /// tighter or looser, and so does a role's line added or removed
    /// (Richard, 2026-10-31).
    #[test]
    fn every_change_to_a_roles_requirement_needs_a_person() {
        let roles = "
role desk.StaffRole.admin
    sign_in SignInMethod.google
    strength AuthenticationStrength.multi_factor

role desk.StaffRole.front_desk
    sign_in SignInMethod.google
";
        let before = format!("{LIBRARY}{roles}");
        let looser = before.replace("    strength AuthenticationStrength.multi_factor\n", "");
        let tighter = before.replace(
            "role desk.StaffRole.front_desk\n    sign_in SignInMethod.google\n",
            "role desk.StaffRole.front_desk\n    sign_in SignInMethod.google\n    strength AuthenticationStrength.multi_factor\n",
        );
        let added = format!("{before}\nrole desk.StaffRole.porter\n");
        let removed = before.replace(
            "\nrole desk.StaffRole.front_desk\n    sign_in SignInMethod.google\n",
            "",
        );
        for (after, role) in [
            (&looser, "desk.StaffRole.admin"),
            (&tighter, "desk.StaffRole.front_desk"),
            (&added, "desk.StaffRole.porter"),
            (&removed, "desk.StaffRole.front_desk"),
        ] {
            assert_eq!(
                widenings(&before, after),
                pairs(&[(role, "PolicyChange.role_requirements_changed")]),
                "{after}"
            );
        }
        assert_eq!(widenings(&before, &before), Vec::<(String, String)>::new());
    }
}
