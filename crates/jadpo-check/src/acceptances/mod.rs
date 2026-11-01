//! Acceptances (area 18, "Acceptances"; chunk A8): a person's decision to
//! allow changes that can break someone else or widen who can do what.
//!
//! [`needs`] compares the records at the before side, which `jadpo check`
//! reads from git ([`jadpo_db::Before`]), with the records today's source
//! writes, and lists every change that needs a person. [`problems`] then
//! judges the acceptance files a change adds: each change needs an added
//! acceptance listing it, each listed entry must match a change, and each
//! added acceptance needs a person's `///` reason. Acceptances already on
//! the before side are history, and editing one is an error.
//!
//! With no before side (not a git repository, no default branch, a
//! project new since its base), nothing is compared or judged, and `jadpo
//! check` says so in one line (Richard, 2026-10-30); only a migration that
//! removes data is still held to an acceptance ([`data::unaccepted`]).

pub mod access;
pub mod compare;
pub mod data;
pub mod files;
pub mod network;
pub mod words;

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::record as format;
use jadpo_db::{Before, BeforeSide, Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};

pub use compare::{Family, Need};

/// Every change since the before side that needs a person, sorted by
/// subject; empty without a before side. A record whose source has errors
/// isn't compared, since what it would say isn't known.
#[salsa::tracked(returns(ref))]
pub fn needs(db: &dyn Db, project: Project) -> Vec<Need> {
    let Before::At(before) = project.before(db) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    api_needs(db, project, before, &mut out);
    event_needs(db, project, before, &mut out);
    if let (Some(was), Some(now)) = (before.text(access::PATH), access::record_text(db, project)) {
        compare::access_needs(access::PATH, was, now.as_deref().unwrap_or(""), &mut out);
    }
    if let (Some(was), Some(now)) = (network_before(before), network::record_text(db, project)) {
        compare::network(network::PATH, was, now.as_deref().unwrap_or(""), &mut out);
    }
    if let Some(was) = before.text(jadpo_db::LOCK) {
        let now = written(db, project, jadpo_db::LOCK).unwrap_or_default();
        compare::lock(jadpo_db::LOCK, was, &now, &mut out);
    }
    compare::domains(&before.domains, &domains(db, project), &mut out);
    data::needs(db, project, before, &mut out);
    out.sort();
    out.dedup_by(|a, b| a.subject == b.subject && a.change == b.change);
    out
}

/// The network record on the before side. A before side with none whose
/// `app.jadpo` has no `connections:` block reached no outside host, so it
/// is read as empty and the change that adds the first host is compared as
/// usual; one with the block and no record is a project from before the
/// record, compared with nothing (2026-10-31).
fn network_before(before: &BeforeSide) -> Option<&str> {
    match before.text(network::PATH) {
        Some(text) => Some(text),
        None if !before_connections(before) => Some(""),
        None => None,
    }
}

/// Whether the before side's `application { }` has a `connections:` block;
/// `true` when its `app.jadpo` can't be read, so nothing is assumed.
fn before_connections(before: &BeforeSide) -> bool {
    use jadpo_syntax::ast::{self, AstNode};
    let Some(text) = before.text(jadpo_db::PROJECT_FILE) else {
        return true;
    };
    let parse = jadpo_syntax::parse_file(text);
    if parse.has_errors() {
        return true;
    }
    parse
        .syntax()
        .children()
        .filter_map(ast::SingletonDecl::cast)
        .filter(|decl| {
            decl.keyword()
                .is_some_and(|word| word.text() == "application")
        })
        .filter_map(|decl| decl.member_list())
        .flat_map(|list| {
            list.syntax()
                .children()
                .filter_map(ast::Member::cast)
                .collect::<Vec<_>>()
        })
        .any(|member| {
            member
                .member_key()
                .is_some_and(|key| key.syntax().text() == "connections")
        })
}

fn written(db: &dyn Db, project: Project, path: &str) -> Option<String> {
    project
        .records(db)
        .iter()
        .find(|record| record.path == path)
        .and_then(|record| record.text.clone().ok())
}

/// The project's domains: each folder under `domains/` holding a file.
fn domains(db: &dyn Db, project: Project) -> BTreeSet<String> {
    project
        .files(db)
        .iter()
        .map(|file| file.path(db).as_str())
        .chain(
            project
                .records(db)
                .iter()
                .map(|record| record.path.as_str()),
        )
        .chain(project.other_files(db).iter().map(String::as_str))
        .filter_map(|path| {
            let rest = path.strip_prefix("domains/")?;
            let (domain, _) = rest.split_once('/')?;
            Some(domain.to_string())
        })
        .collect()
}

fn api_needs(db: &dyn Db, project: Project, before: &BeforeSide, out: &mut Vec<Need>) {
    if !crate::routes::routes_clean(db, project) {
        return;
    }
    let after = crate::routes::record::record_texts(db, project);
    for (path, text) in &before.files {
        let Some((kind, _)) = format::kind_of(path) else {
            continue;
        };
        if kind.word != format::API.word {
            continue;
        }
        let api = path
            .trim_start_matches("api/")
            .trim_end_matches(format::EXTENSION);
        compare::api(api, path, text, after.get(api).map(String::as_str), out);
    }
}

fn event_needs(db: &dyn Db, project: Project, before: &BeforeSide, out: &mut Vec<Need>) {
    use crate::events::record as events;
    if !events::events_clean(db, project) {
        return;
    }
    let after = events::record_texts(db, project);
    for (path, text) in &before.files {
        let Some(domain) = events::domain_of(path) else {
            continue;
        };
        compare::events(
            domain,
            path,
            text,
            after.get(domain).map(String::as_str),
            out,
        );
    }
}

/// The added acceptances: those the before side doesn't have.
fn added(before: &BeforeSide, all: &[files::Acceptance]) -> Vec<files::Acceptance> {
    all.iter()
        .filter(|acceptance| before.text(&acceptance.path).is_none())
        .cloned()
        .collect()
}

/// The changes no added acceptance lists yet, which `jadpo acceptance new`
/// drafts.
pub fn unaccepted(db: &dyn Db, project: Project) -> Vec<Need> {
    let Before::At(before) = project.before(db) else {
        return Vec::new();
    };
    let listed: BTreeSet<(String, String)> = added(before, &files::acceptances(db, project))
        .iter()
        .flat_map(|acceptance| acceptance.entries.iter())
        .map(|entry| (entry.subject.clone(), entry.change.clone()))
        .collect();
    needs(db, project)
        .iter()
        .filter(|need| !listed.contains(&(need.subject.clone(), need.change.clone())))
        .cloned()
        .collect()
}

/// Every problem with the project's acceptances: `acceptance.invalid`
/// always, and with a before side `acceptance.missing`,
/// `acceptance.unmatched`, `acceptance.no_reason` and `acceptance.edited`,
/// or `acceptance.base_missing` when the base commit isn't in the clone.
#[salsa::tracked(returns(ref))]
pub fn problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    if project.layout(db) != jadpo_db::Layout::Project {
        return Vec::new();
    }
    let all = files::acceptances(db, project);
    let mut out: Vec<Diagnostic> = all
        .iter()
        .flat_map(|acceptance| acceptance.problems.iter().cloned())
        .collect();
    let before = match project.before(db) {
        Before::At(before) => before,
        Before::Missing { base } => {
            out.push(base_missing(base));
            return out;
        }
        // With no before side, nothing says what this change added, so a
        // removal is held to any acceptance with a reason (chunk V3).
        Before::Nothing(_) => {
            out.extend(data::unaccepted(db, project));
            return out;
        }
        Before::NotRead => return out,
    };
    for acceptance in &all {
        if let Some(was) = before.text(&acceptance.path)
            && format::normalise(was) != format::normalise(&texts(db, project, &acceptance.path))
        {
            out.push(edited(acceptance, before));
        }
    }
    let added = added(before, &all);
    let needs = needs(db, project);
    let wanted: BTreeSet<(&str, &str)> = needs
        .iter()
        .map(|need| (need.subject.as_str(), need.change.as_str()))
        .collect();
    let mut listed: BTreeSet<(&str, &str)> = BTreeSet::new();
    for acceptance in &added {
        if acceptance.problems.is_empty() && acceptance.reason.is_none() {
            out.push(no_reason(acceptance));
        }
        for entry in &acceptance.entries {
            listed.insert((&entry.subject, &entry.change));
            if !wanted.contains(&(entry.subject.as_str(), entry.change.as_str())) {
                out.push(unmatched(entry, needs));
            }
        }
    }
    // One error per kind of change and file, naming each subject.
    let mut missing: BTreeMap<(String, String, Family), Vec<&Need>> = BTreeMap::new();
    for need in needs {
        if !listed.contains(&(need.subject.as_str(), need.change.as_str())) {
            missing
                .entry((need.path.clone(), need.change.clone(), need.family.clone()))
                .or_default()
                .push(need);
        }
    }
    for ((path, change, family), needs) in missing {
        out.push(missing_acceptance(
            db, project, &path, &change, &family, &needs,
        ));
    }
    out
}

fn texts(db: &dyn Db, project: Project, path: &str) -> String {
    project
        .files(db)
        .iter()
        .find(|file| file.path(db) == path)
        .map(|file| file.text(db).clone())
        .unwrap_or_default()
}

/// What the error says a change does, by the kind of change and the
/// record it shows in.
fn said(change: &str, family: &Family) -> (&'static str, &'static str) {
    let api = *family == Family::Api;
    match change {
        "ApiChange.route_removed" => (
            "removing a route can break clients",
            "Clients that call it get 404 from the next deploy.",
        ),
        "ApiChange.input_field_required" if api => (
            "a new required input field can break clients",
            "Clients that don't send it yet are refused with 400.",
        ),
        "ApiChange.input_field_required" => (
            "a new required field can break the events already waiting",
            "Events in the outbox were written without it, and are read with the new shape.",
        ),
        "ApiChange.rule_tightened" if api => (
            "a tightened rule can refuse what clients send",
            "A value a client sends today may be refused with 400 from the next deploy.",
        ),
        "ApiChange.rule_tightened" => (
            "a tightened rule can refuse the events already waiting",
            "Events in the outbox were written to the old rule, and are read with the new one.",
        ),
        "ApiChange.output_field_removed" if api => (
            "removing an output field can break clients",
            "Clients that read it find it gone.",
        ),
        "ApiChange.output_field_removed" => (
            "removing an event's field can break its subscribers",
            "Subscribers, other domains' among them, read what an event carries.",
        ),
        "ApiChange.output_field_retyped" if api => (
            "retyping an output field can break clients",
            "Clients that read it get values of another kind.",
        ),
        "ApiChange.output_field_retyped" => (
            "retyping an event's field can break its subscribers",
            "Subscribers read what an event carries, and events in the outbox were written with the old type.",
        ),
        "ApiChange.failure_changed" => (
            "changing a failure code can break clients",
            "Clients that handle the failure by its code or status no longer recognise it.",
        ),
        "ApiChange.event_removed" => (
            "removing an event can break its subscribers",
            "Events already in the outbox, and subscribers in other domains, still name it.",
        ),
        "PolicyChange.route_public" => (
            "opening a route to anyone widens who can call it",
            "Anyone at all may now call it, without signing in.",
        ),
        "PolicyChange.widened" => (
            "widening who can do what needs a person",
            "Callers who were refused are now let in.",
        ),
        "PolicyChange.restriction_removed" => (
            "removing a field's restriction widens who can reach it",
            "The field now follows its record's grants, which let more callers in.",
        ),
        "PolicyChange.tenancy_loosened" => (
            "loosening tenancy lets records cross tenants",
            "Records that stayed inside their tenant are now shared by every tenant.",
        ),
        "PolicyChange.shared_entity_added" => (
            "a new entity shared by every tenant needs a person",
            "An entity belongs to a tenant unless a person accepts otherwise.",
        ),
        "PolicyChange.listing_added" => (
            "listing an action replaces its entities' grants",
            "The people its line names may now make its writes, whatever the entities' grants say.",
        ),
        "PolicyChange.listing_removed" => (
            "removing a listing changes who can run it",
            "It now takes its permission from its entities' grants, which may let more callers in.",
        ),
        "PolicyChange.sign_in_loosened" => (
            "loosening sign-in past its limit weakens it",
            "Past its limit, a setting makes guessing or reusing a sign-in link or code easier.",
        ),
        "PolicyChange.role_requirements_changed" => (
            "changing how a role must be signed in to needs a person",
            "Who keeps the role, and where their access is revoked, follows from it, whether it is tighter or looser.",
        ),
        "NetworkChange.host_added" => (
            "a new outside host the app can send data to needs a person",
            "Data the app holds can now leave for a host it couldn't reach before.",
        ),
        "PackageChange.git_source" => (
            "a package installed from git has no verified publisher",
            "Nothing verifies who published a package from git, as the registry does.",
        ),
        "DomainChange.created" => (
            "a new domain needs a person",
            "A domain is a boundary other code is shaped around.",
        ),
        "DataChange.column_dropped" => (
            "dropping a column removes its data for good",
            "Every value it holds goes when the migration runs, and no build can bring it back.",
        ),
        "DataChange.table_dropped" => (
            "dropping a table removes its records for good",
            "Every record it holds goes when the migration runs, and no build can bring it back.",
        ),
        "DataChange.entity_removed" => (
            "removing an entity takes its records out of the app's reach",
            "Its table stays, retired, where no code, erasure or retention reaches its records, and only dropping the table ends that.",
        ),
        "DataChange.entity_moved" => (
            "moving an entity to another store leaves its records behind",
            "Its table starts empty in the new store, and nothing copies the records across.",
        ),
        change if change.ends_with("_declined") => (
            "declining a security rule needs a person",
            "An agent can write a decline's reason itself, so a person accepts each decline of a security rule.",
        ),
        _ => (
            "this change needs a person",
            "It can break someone else, or widen who can do what.",
        ),
    }
}

/// Where a change shows: its entry's line in the record when it has one,
/// else the file.
fn place(db: &dyn Db, project: Project, path: &str, heading: Option<&str>) -> Span {
    let text = written(db, project, path).unwrap_or_default();
    let found = heading.and_then(|heading| {
        let mut start = 0usize;
        for line in text.split_inclusive('\n') {
            if line.trim_end_matches('\n') == heading {
                return Some((start, start + heading.len()));
            }
            start += line.len();
        }
        None
    });
    match found {
        Some((start, end)) => Span::new(path, start as u32, end as u32),
        None => crate::tree::file_span(path),
    }
}

fn missing_acceptance(
    db: &dyn Db,
    project: Project,
    path: &str,
    change: &str,
    family: &Family,
    needs: &[&Need],
) -> Diagnostic {
    let (what, why) = said(change, family);
    let subjects: Vec<String> = needs.iter().map(|need| need.subject.clone()).collect();
    let span = match family {
        Family::Decline => decline_span(db, project, needs[0])
            .unwrap_or_else(|| place(db, project, path, needs[0].heading.as_deref())),
        Family::Data => data::span(db, project, needs[0]),
        _ => place(db, project, path, needs[0].heading.as_deref()),
    };
    Diagnostic::new(
        &rule::ACCEPTANCE_MISSING,
        format!("{what}: {}", subjects.join(", ")),
    )
    .at(span, change)
    .reason(format!(
        "{why} So a person accepts it: no acceptance this change adds lists {}.",
        if subjects.len() == 1 {
            format!("`{}: {change}`", subjects[0])
        } else {
            format!("these as `{change}`")
        }
    ))
    .next("A person accepts it: `jadpo acceptance new <purpose>` writes the draft for them")
}

/// The `decline` line a need for a declined security rule is about.
fn decline_span(db: &dyn Db, project: Project, need: &Need) -> Option<Span> {
    crate::declines::declines(db, project)
        .into_iter()
        .find(|decline| {
            jadpo_diag::find(&decline.rule).is_some_and(|rule| rule.security)
                && decline
                    .subject
                    .as_deref()
                    .is_some_and(|subject| access::qualify(&decline.path, subject) == need.subject)
        })
        .map(|decline| decline.span)
}

fn no_reason(acceptance: &files::Acceptance) -> Diagnostic {
    Diagnostic::new(
        &rule::ACCEPTANCE_NO_REASON,
        format!("`{}` has no reason", acceptance.path),
    )
    .at(acceptance.span.clone(), "no `///` reason above")
    .reason("An acceptance records a person's decision, so it says why on a `///` line above `acceptance`; an agent writes the draft and stops there.")
    .next("A person writes why these changes are accepted on a `///` line above `acceptance`")
}

fn unmatched(entry: &files::Entry, needs: &[Need]) -> Diagnostic {
    let same_subject: Vec<String> = needs
        .iter()
        .filter(|need| need.subject == entry.subject)
        .map(|need| format!("`{}`", need.change))
        .collect();
    let reason = if same_subject.is_empty() {
        "An acceptance allows the changes this change makes, so each entry must name one `jadpo check` reports.".to_string()
    } else {
        format!(
            "An acceptance allows the changes this change makes, so each entry must name one `jadpo check` reports; `{}` changes as {}.",
            entry.subject,
            crate::or_list(&same_subject)
        )
    };
    Diagnostic::new(
        &rule::ACCEPTANCE_UNMATCHED,
        format!("`{}: {}` matches no change", entry.subject, entry.change),
    )
    .at(entry.span.clone(), "nothing changed like this")
    .reason(reason)
    .next(
        "Remove the entry, or run `jadpo acceptance new <purpose>` to list what this change needs",
    )
}

fn edited(acceptance: &files::Acceptance, before: &BeforeSide) -> Diagnostic {
    Diagnostic::new(
        &rule::ACCEPTANCE_EDITED,
        format!("`{}` was accepted before this change", acceptance.path),
    )
    .at(acceptance.span.clone(), "edited since")
    .reason(format!(
        "It is already on the before side ({}), so it is history: it allowed the change that added it, and allows nothing new.",
        before.described
    ))
    .next(format!(
        "Put it back with `git restore --source={} {}`, and list new changes in an acceptance of their own",
        before.commit, acceptance.path
    ))
}

fn base_missing(base: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::ACCEPTANCE_BASE_MISSING,
        format!("The base to compare with, {base}, isn't in this clone"),
    )
    .reason("`jadpo check` compares the change with the default branch at its merge base, and this clone was made without that commit, so what needs a person's acceptance can't be worked out.")
    .next("Fetch the history with `git fetch --unshallow`; in CI, check out with `fetch-depth: 0`")
}

/// The records the before side doesn't have that today's source writes:
/// `access.jadpo_record` or `network.jadpo_record` arriving with this
/// change, whose changes are compared with nothing (Richard, 2026-10-30).
pub fn introduced(db: &dyn Db, project: Project) -> Vec<&'static str> {
    let Before::At(before) = project.before(db) else {
        return Vec::new();
    };
    let access = (before.text(access::PATH).is_none())
        .then_some((access::PATH, access::record_text(db, project)));
    let network = network_before(before)
        .is_none()
        .then_some((network::PATH, network::record_text(db, project)));
    [access, network]
        .into_iter()
        .flatten()
        .filter(|(_, now)| matches!(now, Some(Some(_))))
        .map(|(path, _)| path)
        .collect()
}
