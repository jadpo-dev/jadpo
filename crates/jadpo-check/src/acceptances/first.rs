//! What a first record holds (area 18, "What a first record holds"; chunk
//! A15). A first commit has nothing to compare with, so what would need a
//! person later passes unseen: a new project's public create, update or
//! delete, above all. Each is a notice for a person rather than an error: a
//! decision, not a fault, since an app may mean it. It goes once a commit
//! holds it, and from then on it is compared like anything else.
//!
//! The notices run the comparisons in [`super::compare`] against an empty
//! before side, keeping the kinds that are a decision even as a project's
//! first. The ones that aren't, such as the first routes, grants or domains,
//! are what a project is made of; area 18 says which is which. A public write,
//! and a public read of personal data (chunk A20), has no kind of its own: a
//! new entity's grants need no acceptance, so the first record holding one
//! says so, however old the record.

use std::collections::BTreeSet;

use jadpo_db::record as format;
use jadpo_db::{Before, BeforeSide, Db, Layout, Project};

use super::compare::{self, Need};
use super::{access, files, needs, network, network_before, words, written};
use crate::lookups::Lookup;
use crate::types::TypeView;

/// A notice's kind when no acceptance names it: a public create, update or
/// delete.
pub const PUBLIC_WRITE: &str = "public_write";

/// A notice's kind when no acceptance names it: personal data anyone may
/// read.
pub const PUBLIC_READ: &str = "public_read";

/// One thing a first record holds that a person decides.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Notice {
    pub subject: String,
    /// [`PUBLIC_WRITE`], [`PUBLIC_READ`], or the change an acceptance would
    /// name.
    pub kind: String,
    /// In plain words, as an acceptance's message would say it.
    pub said: String,
    /// The file it shows in, and the line there, when there is one.
    pub path: String,
    pub heading: Option<String>,
    pub line: Option<i64>,
}

impl Notice {
    /// Whether `jadpo check` refuses it from the next change until a person
    /// accepts it, since it says how things stand rather than what changed.
    pub fn refused_next(&self) -> bool {
        self.kind == crate::provider_erasure::DATA_STAYS
    }

    /// Where it shows: `access.jadpo_record:4`.
    pub fn place(&self) -> String {
        match self.line {
            Some(line) => format!("{}:{line}", self.path),
            None => self.path.clone(),
        }
    }
}

/// What this change holds that nothing compared; empty with a before side
/// that holds it, and outside git, where no change is being reviewed.
#[salsa::tracked(returns(ref))]
pub fn notice_list(db: &dyn Db, project: Project) -> Vec<Notice> {
    if project.layout(db) != Layout::Project {
        return Vec::new();
    }
    let side = match project.before(db) {
        Before::At(side) => Some(side),
        Before::Nothing(_) => None,
        Before::Missing { .. } | Before::NotRead => return Vec::new(),
    };
    let mut out = Vec::new();
    if let Some(Some(now)) = access::record_text(db, project) {
        from_access(db, project, side, now, &mut out);
    }
    if let Some(Some(now)) = network::record_text(db, project)
        && side.is_none_or(|side| network_before(side).is_none())
    {
        from_network(now, &mut out);
    }
    if side.is_none() {
        let now = written(db, project, jadpo_db::LOCK).unwrap_or_default();
        let own = &crate::own_sources::own_sources(db, project).entries;
        let mut found = Vec::new();
        compare::lock(jadpo_db::LOCK, "", &now, own, &mut found);
        out.extend(found.iter().map(|need| from_need(need, &now)));
        from_erasure(db, project, &mut out);
    }
    // What is judged is said as a decision, and what a person has given a
    // reason for is decided.
    let judged: BTreeSet<(&str, &str)> = needs(db, project)
        .iter()
        .map(|need| (need.subject.as_str(), need.change.as_str()))
        .collect();
    let decided: BTreeSet<(String, String)> = files::acceptances(db, project)
        .into_iter()
        .filter(|acceptance| acceptance.reason.is_some() && acceptance.problems.is_empty())
        .flat_map(|acceptance| acceptance.entries)
        .map(|entry| (entry.subject, entry.change))
        .collect();
    out.retain(|notice| {
        !judged.contains(&(notice.subject.as_str(), notice.kind.as_str()))
            && !decided.contains(&(notice.subject.clone(), notice.kind.clone()))
    });
    out.sort();
    out.dedup();
    out
}

fn from_need(need: &Need, text: &str) -> Notice {
    Notice {
        subject: need.subject.clone(),
        kind: need.change.clone(),
        said: need.said.clone(),
        path: need.path.clone(),
        line: need
            .heading
            .as_deref()
            .and_then(|heading| line_of(text, heading)),
        heading: need.heading.clone(),
    }
}

/// The line of `text` that is `heading`, from 1.
fn line_of(text: &str, heading: &str) -> Option<i64> {
    text.lines()
        .position(|line| line == heading)
        .map(|at| at as i64 + 1)
}

fn from_access(
    db: &dyn Db,
    project: Project,
    side: Option<&BeforeSide>,
    now: &str,
    out: &mut Vec<Notice>,
) {
    let was = side.and_then(|side| side.text(access::PATH));
    if was.is_none_or(|was| !holds_public_write(db, project, was)) {
        public_writes(db, project, now, out);
    }
    if was.is_none_or(|was| readable_personal(db, project, was).is_empty()) {
        public_reads(db, project, now, out);
    }
    if was.is_some() {
        return;
    }
    let mut found = Vec::new();
    compare::access_needs(access::PATH, "", now, &mut found);
    if side.is_none() {
        compare::own_source_needs(access::PATH, "", now, &mut found);
    }
    out.extend(
        found
            .iter()
            .filter(|need| decided_even_when_first(&need.change))
            .map(|need| from_need(need, now)),
    );
}

/// The kinds a record's comparison gives that are a decision even as a
/// project's first. The others, a role's requirement or an action's line,
/// are how a project says who can do what at all (area 18).
fn decided_even_when_first(change: &str) -> bool {
    change == "PolicyChange.shared_entity_added"
        || change == "PolicyChange.sign_in_loosened"
        || change == "PolicyChange.own_source_added"
        || change.ends_with("_declined")
}

/// Each entity with a create, update or delete anyone may make, and each
/// action anyone may run that writes, in the access record's `text`.
fn public_writes(db: &dyn Db, project: Project, text: &str, out: &mut Vec<Notice>) {
    let judged = needs(db, project);
    let widened = |subject: &str| judged.iter().any(|need| need.subject == subject);
    for (entity, effects) in compare::public_writes(text) {
        let effects: Vec<&str> = effects
            .iter()
            .filter(|effect| !widened(&format!("{entity}.{effect}")))
            .map(String::as_str)
            .collect();
        if effects.is_empty() {
            continue;
        }
        let heading = format!("entity {entity}");
        out.push(Notice {
            said: format!(
                "anyone can {} `{entity}` records, with no sign-in",
                words::verbs(&effects)
            ),
            subject: entity,
            kind: PUBLIC_WRITE.to_string(),
            path: access::PATH.to_string(),
            line: line_of(text, &heading),
            heading: Some(heading),
        });
    }
    for action in public_actions(db, project, text) {
        if widened(&action) {
            continue;
        }
        let heading = format!("action {action}");
        out.push(Notice {
            said: format!(
                "anyone can run `{action}`, which creates, changes or deletes records, with no sign-in"
            ),
            subject: action,
            kind: PUBLIC_WRITE.to_string(),
            path: access::PATH.to_string(),
            line: line_of(text, &heading),
            heading: Some(heading),
        });
    }
}

/// Each entity anyone may read, in the access record's `text`, that holds
/// personal data they can see, as the personal-data report counts it: a field
/// marked `personal:`, or of a standard type that is personal data. By the
/// name the record gives it, with those fields' names.
fn readable_personal(db: &dyn Db, project: Project, text: &str) -> Vec<(String, Vec<String>)> {
    let access = crate::policy::access(db, project);
    let lookup = Lookup::new(db, project);
    let marks = crate::ai_personal::marks(db, project);
    let reads = compare::public_reads(text);
    let mut out = Vec::new();
    for policy in access.entities.values() {
        let written = crate::policy::report::qualified(&lookup, &policy.entity);
        let Some(kept) = reads.get(&written) else {
            continue;
        };
        let mut fields: Vec<String> = lookup
            .fields(&policy.entity)
            .into_iter()
            .flatten()
            .filter(|field| !kept.contains(&field.name))
            .filter(|field| {
                crate::ai_personal::field_holding(&lookup, marks, &policy.entity, &field.name)
                    .is_some_and(|holding| !holding.categories.is_empty())
            })
            .map(|field| field.name.clone())
            .collect();
        fields.sort();
        if !fields.is_empty() {
            out.push((written, fields));
        }
    }
    out
}

/// Each entity with personal data anyone may read, in the access record's
/// `text`. What is judged, a widening or a restriction taken away, is said as
/// a decision rather than here.
fn public_reads(db: &dyn Db, project: Project, text: &str, out: &mut Vec<Notice>) {
    let judged = needs(db, project);
    let is_judged = |subject: &str| judged.iter().any(|need| need.subject == subject);
    for (entity, fields) in readable_personal(db, project, text) {
        if is_judged(&format!("{entity}.read")) {
            continue;
        }
        let named: Vec<String> = fields
            .into_iter()
            .map(|field| format!("{entity}.{field}"))
            .filter(|field| !is_judged(field) && !is_judged(&format!("{field}.read")))
            .map(|field| format!("`{field}`"))
            .collect();
        if named.is_empty() {
            continue;
        }
        let heading = format!("entity {entity}");
        out.push(Notice {
            said: format!(
                "anyone can read {}, personal data, with no sign-in",
                crate::and_list(&named)
            ),
            subject: entity,
            kind: PUBLIC_READ.to_string(),
            path: access::PATH.to_string(),
            line: line_of(text, &heading),
            heading: Some(heading),
        });
    }
}

/// Whether the access record's `text` holds a public write already.
fn holds_public_write(db: &dyn Db, project: Project, text: &str) -> bool {
    !compare::public_writes(text).is_empty() || !public_actions(db, project, text).is_empty()
}

/// The actions under `actions:` in `text` that anyone may run and that write,
/// themselves or through what they call, as the record names them.
fn public_actions(db: &dyn Db, project: Project, text: &str) -> Vec<String> {
    let access = crate::policy::access(db, project);
    compare::public_actions(text)
        .into_iter()
        .filter(|written| {
            access
                .root
                .actions
                .iter()
                .find(|line| line.written == *written)
                .is_some_and(|line| crate::policy::reach::writes(db, project, &line.action))
        })
        .collect()
}

/// Each connection that reaches an outside host. The app's own database and
/// cache aren't a decision at first: every app has them, and what leaves goes
/// to a host someone else runs.
fn from_network(now: &str, out: &mut Vec<Notice>) {
    let own_stores: BTreeSet<String> = compare::entries(now, &format::NETWORK)
        .into_iter()
        .filter(|entry| {
            entry
                .members
                .iter()
                .any(|member| member.line == "for database" || member.line == "for cache")
        })
        .map(|entry| entry.heading)
        .collect();
    let mut found = Vec::new();
    compare::network(network::PATH, "", now, &mut found);
    out.extend(
        found
            .iter()
            .filter(|need| {
                need.heading
                    .as_ref()
                    .is_none_or(|heading| !own_stores.contains(heading))
            })
            .map(|need| from_need(need, now)),
    );
}

/// Each connection that keeps personal data after a person is erased: read
/// from what the app sends, as [`super::erasure_needs`] reads it once there
/// is a before side.
fn from_erasure(db: &dyn Db, project: Project, out: &mut Vec<Notice>) {
    use crate::provider_erasure::{self, DATA_STAYS};
    if !provider_erasure::clean(db, project) {
        return;
    }
    for unerased in provider_erasure::unerased(db, project) {
        let span = provider_erasure::connection_span(db, project, &unerased.connection);
        let text = super::texts(db, project, jadpo_db::PROJECT_FILE);
        out.push(Notice {
            said: unerased.said(),
            subject: unerased.connection.clone(),
            kind: DATA_STAYS.to_string(),
            path: jadpo_db::PROJECT_FILE.to_string(),
            heading: None,
            line: span.and_then(|span| {
                let before = text.get(..span.start as usize)?;
                Some(before.matches('\n').count() as i64 + 1)
            }),
        });
    }
}
