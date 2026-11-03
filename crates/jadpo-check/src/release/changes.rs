//! A release's change record (area 11, "The changelog"): each entry names a
//! declaration and what happened to it. `CHANGELOG.md`'s list is written
//! from it, and the author writes only why.
//!
//! What the surface shows comes from comparing the two surface records,
//! [`surface_changes`]; where personal data goes from comparing the two
//! releases' facts, [`personal_data_changes`]; and what the types don't
//! show from running the previous release's examples and tests against the
//! new code, each one that fails a [`behaviour_changed`] entry.

use jadpo_db::record::{self as format, Entry};

use super::facts::Facts;
use crate::publishing::significant;

/// What happened to a declaration in a release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Added,
    Removed,
    /// What it takes, gives or says changed.
    Retyped,
    NewFailure,
    NewHost,
    PersonalDataSent,
    BehaviourChanged,
}

impl Change {
    /// As the registry's protocol names it.
    pub fn name(self) -> &'static str {
        match self {
            Change::Added => "added",
            Change::Removed => "removed",
            Change::Retyped => "retyped",
            Change::NewFailure => "new_failure",
            Change::NewHost => "new_host",
            Change::PersonalDataSent => "personal_data_sent",
            Change::BehaviourChanged => "behaviour_changed",
        }
    }

    /// As a screen says it.
    pub fn word(self) -> &'static str {
        match self {
            Change::Added => "added",
            Change::Removed => "removed",
            Change::Retyped => "retyped",
            Change::NewFailure => "new failure",
            Change::NewHost => "new host",
            Change::PersonalDataSent => "personal data",
            Change::BehaviourChanged => "behaviour changed",
        }
    }
}

/// One entry of a change record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeEntry {
    /// As the app writes it after the package's name: `uk.parse`, or
    /// `provider` and the package's name for those two entries.
    pub declaration: String,
    /// What the declaration is: `function`, `type`, `provider`.
    pub kind: String,
    pub change: Change,
    /// What changed about it: the failure, the host, the field and where
    /// it now goes, the new facts and the old, or the example that failed.
    pub detail: Option<String>,
    /// The author's reason, where the change needs one: a behaviour change
    /// marked a fix.
    pub why: Option<String>,
}

impl ChangeEntry {
    fn new(declaration: &str, kind: &str, change: Change, detail: Option<String>) -> ChangeEntry {
        ChangeEntry {
            declaration: declaration.to_string(),
            kind: kind.to_string(),
            change,
            detail,
            why: None,
        }
    }

    /// Its line in `CHANGELOG.md`'s list.
    pub fn line(&self) -> String {
        let (kind, name) = (&self.kind, &self.declaration);
        let detail = self.detail.as_deref().unwrap_or_default();
        match self.change {
            Change::Added => format!("added {kind} `{name}`"),
            Change::Removed => format!("removed {kind} `{name}`"),
            Change::Retyped if kind == "provider" => "changed provider".to_string(),
            Change::Retyped if kind == "package" => format!("changed package `{name}`"),
            Change::Retyped => format!("retyped {kind} `{name}`"),
            Change::NewFailure => format!("new failure `{detail}` on {kind} `{name}`"),
            Change::NewHost => format!("new host `{detail}`"),
            Change::PersonalDataSent => {
                format!("personal data sent by {kind} `{name}`: {detail}")
            }
            Change::BehaviourChanged => format!("behaviour changed `{name}`: {detail}"),
        }
    }
}

/// A previous release's example or test, `test`, failing against the new
/// code, on the declaration it calls.
pub fn behaviour_changed(declaration: &str, kind: &str, test: &str) -> ChangeEntry {
    ChangeEntry::new(
        declaration,
        kind,
        Change::BehaviourChanged,
        Some(test.to_string()),
    )
}

/// What an entry's heading says it is, and its name: `("type", "UkPostcode")`
/// from `type UkPostcode = Text`, `("provider", "provider")`.
fn parts(heading: &str) -> (&str, &str) {
    let named = heading.split(" = ").next().unwrap_or(heading);
    named.split_once(' ').unwrap_or((named, named))
}

/// An entry's lines an app relies on, a type's base among them.
fn lines(entry: &Entry) -> Vec<String> {
    let mut out = significant(entry);
    if let Some((_, base)) = entry.heading.split_once(" = ") {
        out.push(format!("= {base}"));
    }
    out
}

/// How the surface changed from `before` to `now`, entry by entry in the
/// record's order, those removed last. Descriptions, files and versions
/// alone aren't changes.
pub fn surface_changes(before: &str, now: &str) -> Vec<ChangeEntry> {
    let read =
        |text: &str| format::read(&format::normalise(text), &format::SURFACE).unwrap_or_default();
    let (before, now) = (read(before), read(now));
    // A new major version's package has its own name.
    let same = |a: &Entry, b: &Entry| {
        let (a, b) = (parts(&a.heading), parts(&b.heading));
        a.0 == b.0 && (a.1 == b.1 || a.0 == "package")
    };
    let mut out = Vec::new();
    for entry in &now {
        let (kind, name) = parts(&entry.heading);
        match before.iter().find(|old| same(old, entry)) {
            None => out.push(ChangeEntry::new(name, kind, Change::Added, None)),
            Some(old) => changed(kind, name, &lines(old), &lines(entry), &mut out),
        }
    }
    for old in &before {
        if !now.iter().any(|entry| same(old, entry)) {
            let (kind, name) = parts(&old.heading);
            out.push(ChangeEntry::new(name, kind, Change::Removed, None));
        }
    }
    out
}

/// One entry's differences: whatever isn't a new failure or a new host is
/// one `retyped`, saying what it says now and what it said.
fn changed(kind: &str, name: &str, old: &[String], new: &[String], out: &mut Vec<ChangeEntry>) {
    let trimmed = |line: &String| line.trim().to_string();
    let removed: Vec<String> = old
        .iter()
        .filter(|line| !new.contains(line))
        .map(trimmed)
        .collect();
    let mut added: Vec<String> = Vec::new();
    let (mut failures, mut hosts) = (Vec::new(), Vec::new());
    for line in new.iter().filter(|line| !old.contains(line)) {
        if let Some(failure) = line.strip_prefix("fails ") {
            failures.push(failure.to_string());
        } else if let Some(host) = new_host(kind, line) {
            hosts.push(host.to_string());
        } else {
            added.push(trimmed(line));
        }
    }
    if !added.is_empty() || !removed.is_empty() {
        let detail = match (added.is_empty(), removed.is_empty()) {
            (false, false) => format!("{}, was {}", added.join(", "), removed.join(", ")),
            (false, true) => added.join(", "),
            _ => format!("no longer {}", removed.join(", ")),
        };
        out.push(ChangeEntry::new(name, kind, Change::Retyped, Some(detail)));
    }
    for failure in failures {
        out.push(ChangeEntry::new(
            name,
            kind,
            Change::NewFailure,
            Some(failure),
        ));
    }
    for host in hosts {
        out.push(ChangeEntry::new(name, kind, Change::NewHost, Some(host)));
    }
}

/// The host a provider entry's line names, when it names one.
fn new_host<'a>(kind: &str, line: &'a str) -> Option<&'a str> {
    if kind != "provider" {
        return None;
    }
    if let Some(rest) = line.strip_prefix("host ") {
        return rest.split_once(' ').map(|(_, host)| host);
    }
    line.strip_prefix("runs asks ")
}

/// Each personal-data field going to a host it didn't reach in the last
/// release, once per field and host, on the first operation that sends it.
/// With no facts of the last release (a release this compiler can't read),
/// everything counts as new, so nothing is hidden.
pub fn personal_data_changes(before: Option<&Facts>, now: &Facts) -> Vec<ChangeEntry> {
    let mut seen: Vec<(&str, &str)> = before
        .map(|facts| {
            facts
                .personal_data
                .iter()
                .map(|sent| (sent.field.as_str(), sent.host.as_str()))
                .collect()
        })
        .unwrap_or_default();
    let mut out = Vec::new();
    for sent in &now.personal_data {
        let pair = (sent.field.as_str(), sent.host.as_str());
        if seen.contains(&pair) {
            continue;
        }
        seen.push(pair);
        out.push(ChangeEntry::new(
            &sent.operation,
            "operation",
            Change::PersonalDataSent,
            Some(format!("{} to {}", sent.field, sent.host)),
        ));
    }
    out
}
