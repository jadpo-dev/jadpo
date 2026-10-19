//! The event record (areas 12 and 18, "Event records", decided
//! 2026-10-19; chunk 6.1c): each domain's committed record of its events,
//! `domains/<domain>/events/events.jadpo_record`, in the record format
//! (`record events 1`). An event's shape is recorded and diffed exactly as
//! an API's is: `jadpo check` fails while the record disagrees with the
//! events (`record.out_of_date`, [`crate::records`]) and `jadpo records
//! update` rewrites it, so every change to what an event carries is a
//! diff of one file. Accepting a breaking change waits for area 18's
//! acceptances, as the API record's does.
//!
//! An entry `event <Name>` says whether other domains may subscribe to it
//! (`public`) and lists its fields as its payload carries them: the wire
//! form the outbox keeps (`Text { max_length: 200 }`), with an entry for
//! each object or enum a field names, as the API record writes them.
//! Events, fields and types sort by born name, so reordering source
//! changes nothing.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::record::{self as format, Entry};
use jadpo_db::{Db, Folder, Project, location};
use jadpo_graph::{Base, Detail, Key, Kind};

use crate::routes::record::{born_order, field_line, sorted_fields, type_entries};
use crate::routes::wire::Wire;

/// A domain's event record file:
/// `domains/planning/events/events.jadpo_record`.
pub fn path(domain: &str) -> String {
    format!("domains/{domain}/events/{}", format::EVENTS_FILE)
}

/// What a domain's event record header says it is generated from.
pub fn from(domain: &str) -> String {
    format!("the events of domain {domain}")
}

/// The domain whose event record is at `path`, when it is one.
pub fn domain_of(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("domains/")?;
    let (domain, file) = rest.split_once('/')?;
    (file == format!("events/{}", format::EVENTS_FILE) && !domain.is_empty()).then_some(domain)
}

/// Whether every file in a domain's `events/` folder reads cleanly, so
/// the record can be written from them.
#[salsa::tracked]
pub fn events_clean(db: &dyn Db, project: Project) -> bool {
    project.files(db).iter().all(|&file| {
        location(db, project, file).folder() != Some(Folder::Events)
            || (crate::resolve_file(db, project, file).is_empty()
                && crate::types::type_problems(db, project, file).is_empty())
    })
}

/// Each domain's event record, by domain, for every domain that declares
/// an event. Empty unless every event reads cleanly.
#[salsa::tracked(returns(ref))]
pub fn record_texts(db: &dyn Db, project: Project) -> BTreeMap<String, String> {
    if !events_clean(db, project) {
        return BTreeMap::new();
    }
    let graph = crate::graph::graph(db, project);
    let wire = Wire {
        graph,
        locales: crate::locales::locales(db, project).plan.as_ref(),
    };
    let mut by_domain: BTreeMap<String, Vec<&Key>> = BTreeMap::new();
    for node in graph.nodes() {
        if node.kind != Kind::Type
            || !matches!(
                node.detail,
                Detail::Type {
                    base: Base::Event,
                    ..
                }
            )
        {
            continue;
        }
        if let Some(domain) = &node.domain {
            by_domain.entry(domain.clone()).or_default().push(&node.key);
        }
    }
    by_domain
        .into_iter()
        .map(|(domain, mut events)| {
            events.sort_by_cached_key(|key| born_order(&wire, key));
            let mut refs: BTreeSet<Key> = BTreeSet::new();
            let mut entries = Vec::new();
            for key in events {
                let mut entry = Entry::new(format!("event {}", wire.name(key)));
                // Another domain may subscribe to it: making it private
                // breaks them.
                if wire.node(key).is_some_and(|node| node.public) {
                    entry.push("public");
                }
                for field in sorted_fields(&wire, wire.fields(key)) {
                    if let Some(line) = field_line(&wire, field, &mut refs) {
                        entry.push(line);
                    }
                }
                entries.push(entry);
            }
            entries.extend(type_entries(&wire, refs).into_values());
            let text = format::write(&format::EVENTS, &from(&domain), &entries);
            (domain, text)
        })
        .collect()
}
