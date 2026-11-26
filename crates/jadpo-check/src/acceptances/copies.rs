//! Field copies a person approved (chunk 10.7c, area 9 "What else shows
//! a field"): a value of a field stored in another field that more people
//! may read is `policy.output_field_unproved`, unless an acceptance names
//! that field flow, `"desk.Macro.body -> desk.Reply.body":
//! PolicyChange.field_copied`. The approval is of the flow, from any
//! action, as a grant is of every action that uses it; another source or
//! another target needs its own.
//!
//! Every acceptance file is read, those on the before side too, since an
//! approved copy goes on being allowed while its acceptance is kept, as a
//! decline goes on declining while its line is there. `access.jadpo_record`
//! keeps each approved copy as a `copy` line, so the acceptance that adds
//! one matches the line it adds, and a reviewer sees it with the grants.

use std::collections::BTreeSet;

use jadpo_db::record as format;
use jadpo_db::{Db, Layout, Project};

/// The change an acceptance lists to approve a copy.
pub const CHANGE: &str = "PolicyChange.field_copied";

/// The access record's heading for a copy: `copy desk.Macro.body ->
/// desk.Reply.body`.
pub const HEADING: &str = "copy ";

/// The subject naming the copy of `from` into `to`, each
/// `domain.Entity.field`, as an acceptance writes it.
pub fn subject(from: &str, to: &str) -> String {
    format::quote(&flow(from, to))
}

/// `from -> to`, as a subject or a record line holds it.
pub fn flow(from: &str, to: &str) -> String {
    format!("{from} -> {to}")
}

/// The source field and the target a subject names, when it names a
/// copy: a field, or a failure a guard raises on the field (chunk 10.11b,
/// `"desk.Customer.blocked -> desk.CustomerBlocked"`).
pub fn parse(subject: &str) -> Option<(String, String)> {
    let text = subject.strip_prefix('"')?.strip_suffix('"')?;
    let (from, to) = text.split_once(" -> ")?;
    let field = |name: &str| name.split('.').count() >= 3 && !name.contains(' ');
    (field(from) && (field(to) || failure(to))).then(|| (from.to_string(), to.to_string()))
}

/// Whether a copy's target names a failure, `desk.CustomerBlocked`, not a
/// field: whether a guard raises it shows one fact about the source.
pub fn failure(to: &str) -> bool {
    let parts: Vec<&str> = to.split('.').collect();
    parts.len() == 2
        && !parts[0].is_empty()
        && !to.contains(' ')
        && parts[1].chars().next().is_some_and(char::is_uppercase)
}

/// Every copy an acceptance in the project approves, as `(from, to)`.
pub fn accepted(db: &dyn Db, project: Project) -> BTreeSet<(String, String)> {
    if project.layout(db) != Layout::Project {
        return BTreeSet::new();
    }
    super::files::acceptances(db, project)
        .iter()
        .filter(|acceptance| acceptance.problems.is_empty())
        .flat_map(|acceptance| acceptance.entries.iter())
        .filter(|entry| entry.change == CHANGE)
        .filter_map(|entry| parse(&entry.subject))
        .collect()
}
