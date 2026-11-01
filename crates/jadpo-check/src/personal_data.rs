//! The personal-data report (area 15, "The personal-data report"; chunk
//! V5): every connection personal data leaves through, the hosts that
//! connection reaches, and each holder with how it goes: what
//! [`crate::policy::flow::exits`] found, written in the form of the other
//! reports, `build/report/personal-data.md` and its `.json` twin. A
//! package's own, what its operations send to its hosts, is what `jadpo
//! package check` shows ([`package_sends`]).
//!
//! The rest of what area 15 lists for each person entity is chunk P8's,
//! which adds to [`report`]: the sections it writes follow the one here,
//! and its JSON sits beside `connections`. Whether each connection can
//! erase its copy is chunk V6's, which reads [`sent`].

use std::collections::BTreeMap;

use jadpo_db::{Db, Project};
use jadpo_graph::json::Value;

use crate::ai_personal::{self, Holder};
use crate::lookups::{Lookup, Name};
pub use crate::policy::flow::exits::{Exit, Left, Through};

/// One holder of personal data leaving through one connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sent {
    pub connection: String,
    pub holder: Holder,
    /// Its categories, in order.
    pub categories: Vec<String>,
    pub through: Through,
    /// Whether it only decided what was sent, rather than being in it.
    pub decided: bool,
}

/// `holder` as a report names it: an app's event by its own name, not the
/// domain a table reads it with, and a package's types through the package.
fn named(db: &dyn Db, project: Project, holder: &Holder) -> Holder {
    match holder {
        Holder::Field { host, field } => {
            let domains = &jadpo_db::symbols(db, project).domains;
            let host = match host.split_once('.') {
                Some((domain, bare)) if domains.contains(domain) => bare.to_string(),
                _ => host.clone(),
            };
            Holder::Field {
                host,
                field: field.clone(),
            }
        }
        other => other.clone(),
    }
}

/// The categories `holder` is.
fn categories(db: &dyn Db, project: Project, holder: &Holder) -> Vec<String> {
    match holder {
        Holder::Field { host, field } => {
            let lookup = Lookup::new(db, project);
            let marks = ai_personal::marks(db, project);
            ai_personal::field_holding(&lookup, marks, host, field)
                .map(|holding| holding.categories.into_keys().collect())
                .unwrap_or_default()
        }
        Holder::Type(ty) => ai_personal::standard_category(ty)
            .map(|category| vec![category.to_string()])
            .unwrap_or_default(),
    }
}

/// Everything that leaves through a connection, by connection, holder and
/// how it goes; what only decided what was sent is dropped where the same
/// holder is also sent the same way.
#[salsa::tracked(returns(ref))]
pub fn sent(db: &dyn Db, project: Project) -> Vec<Sent> {
    let left = crate::policy::flow::exits::left(db, project);
    let mut out: Vec<Sent> = Vec::new();
    for (exit, holder, decided) in left {
        if *decided && left.contains(&(exit.clone(), holder.clone(), false)) {
            continue;
        }
        let sent = Sent {
            connection: exit.connection.clone(),
            holder: named(db, project, holder),
            categories: categories(db, project, holder),
            through: exit.through.clone(),
            decided: *decided,
        };
        if !out.contains(&sent) {
            out.push(sent);
        }
    }
    out.sort_by(|a, b| {
        (&a.connection, &a.holder, &a.through, a.decided).cmp(&(
            &b.connection,
            &b.holder,
            &b.through,
            b.decided,
        ))
    });
    out
}

/// A report: Markdown, and its JSON twin.
#[derive(Debug)]
pub struct Report {
    pub markdown: String,
    pub json: Value,
}

/// What `holder` is, for a report: `` `User.email` ``, `` a `PhoneNumber` ``.
fn shown(holder: &Holder) -> String {
    match holder {
        Holder::Field { host, field } => format!("`{host}.{field}`"),
        Holder::Type(ty) => format!("a `{ty}`"),
    }
}

/// `contact`, or `health`, sensitive.
fn category_words(categories: &[String]) -> String {
    categories
        .iter()
        .map(|category| {
            if ai_personal::is_sensitive(category) {
                format!("`{category}`, sensitive")
            } else {
                format!("`{category}`")
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// How it goes, for a report.
fn through_words(through: &Through) -> String {
    match through {
        Through::Operation(name) => format!("operation `{name}`"),
        Through::Action(name) => format!("action `{name}`"),
        Through::File => "a file in the store".to_string(),
        Through::Prompt(name) => format!("prompt `{name}`"),
        Through::Embedding => "a text sent for an embedding".to_string(),
        Through::Email(name) => format!("email `{name}`"),
        Through::Reply(name) => format!("the reply of `{name}`"),
    }
}

/// The report, for a project that has no errors.
pub fn report(db: &dyn Db, project: Project) -> Report {
    let rows = crate::connections::hosts(db, project, false);
    let mut by_connection: BTreeMap<&str, Vec<&Sent>> = BTreeMap::new();
    for sent in sent(db, project) {
        by_connection
            .entry(&sent.connection)
            .or_default()
            .push(sent);
    }
    let mut markdown = String::from(
        "# Personal data\n\n\
         Where the app's personal data goes: each connection a field marked `personal:`, or a value of a standard type that is personal data, is sent through, and the hosts that connection reaches. A value counts as sent when it may be in what goes out, may have decided it, or was sent to a provider whose answer it may be in. Written by `jadpo check`.\n\n\
         Not listed yet: what each store and cache keeps, who may read it and how long it is kept.\n\n",
    );
    if by_connection.is_empty() {
        markdown.push_str("No personal data leaves through a connection.\n");
    }
    let mut connections = Vec::new();
    for (name, sends) in &by_connection {
        let hosts: Vec<&crate::connections::Row> =
            rows.iter().filter(|row| row.connection == *name).collect();
        markdown.push_str(&format!("## `{name}`\n\n"));
        if hosts.is_empty() {
            markdown.push_str("Reaches no outside host.\n\n");
        } else {
            for row in &hosts {
                match &row.when {
                    Some((setting, word)) => markdown.push_str(&format!(
                        "- {} when `config.{setting}` is `{word}`\n",
                        row.host
                    )),
                    None => markdown.push_str(&format!("- {}\n", row.host)),
                }
            }
            markdown.push('\n');
        }
        markdown.push_str("| Personal data | Category | Sent by | How |\n|---|---|---|---|\n");
        let mut fields = Vec::new();
        for sent in sends {
            markdown.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                shown(&sent.holder),
                category_words(&sent.categories),
                through_words(&sent.through),
                if sent.decided {
                    "decided what was sent"
                } else {
                    "sent"
                }
            ));
            let mut entry = Value::object();
            match &sent.holder {
                Holder::Field { host, field } => {
                    entry.set("host", host.as_str());
                    entry.set("field", field.as_str());
                }
                Holder::Type(ty) => entry.set("type", ty.as_str()),
            }
            entry.set("categories", sent.categories.clone());
            entry.set(
                "sensitive",
                sent.categories
                    .iter()
                    .any(|category| ai_personal::is_sensitive(category)),
            );
            entry.set("through", sent.through.kind());
            if let Some(named) = through_name(&sent.through) {
                entry.set("name", named);
            }
            entry.set("decided_only", sent.decided);
            fields.push(entry);
        }
        markdown.push('\n');
        let mut connection = Value::object();
        connection.set("connection", *name);
        let mut host_rows = Vec::new();
        for row in &hosts {
            let mut host = Value::object();
            if let Some(address) = &row.address {
                host.set("address", address.as_str());
            }
            if let Some(setting) = &row.setting {
                host.set("setting", setting.as_str());
            }
            if let Some(image) = &row.image {
                host.set("service", image.as_str());
            }
            if let Some((setting, word)) = &row.when {
                let mut when = Value::object();
                when.set("setting", setting.as_str());
                when.set("is", *word);
                host.set("when", when);
            }
            host_rows.push(host);
        }
        connection.set("hosts", host_rows);
        connection.set("sent", fields);
        connections.push(connection);
    }
    while markdown.ends_with("\n\n") {
        markdown.pop();
    }
    let mut json = Value::object();
    json.set("connections", connections);
    Report { markdown, json }
}

/// What an exit is named, in the JSON twin; none for those with no name.
fn through_name(through: &Through) -> Option<&str> {
    match through {
        Through::Operation(name)
        | Through::Action(name)
        | Through::Prompt(name)
        | Through::Email(name)
        | Through::Reply(name) => Some(name),
        Through::File | Through::Embedding => None,
    }
}

/// One thing a package's operation sends its hosts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageSend {
    /// `customers.create`.
    pub operation: String,
    pub holder: Holder,
    pub categories: Vec<String>,
    pub decided: bool,
}

/// What the package in `folder` sends through its operations, by operation
/// and holder ([`crate::policy::flow::exits::package_left`]), and the
/// hosts it sends them to: `jadpo package check`'s facts, the data R3's
/// facts print and the registry shows.
pub fn package_sends(
    db: &dyn Db,
    project: Project,
    folder: &str,
) -> (Vec<String>, Vec<PackageSend>) {
    let left =
        crate::policy::flow::exits::package_left(db, project, Name::new(db, folder.to_string()));
    let sends = left
        .iter()
        .filter(|(exit, holder, decided)| {
            !*decided || !left.contains(&(exit.clone(), holder.clone(), false))
        })
        .filter_map(|(exit, holder, decided)| match &exit.through {
            Through::Operation(operation) => Some(PackageSend {
                operation: operation.clone(),
                holder: named(db, project, holder),
                categories: categories(db, project, holder),
                decided: *decided,
            }),
            _ => None,
        })
        .collect();
    let hosts = crate::surface::homes(db, project)
        .get(folder)
        .and_then(|home| crate::connections::reach(db, project, home))
        .map(|reach| {
            let mut hosts = Vec::new();
            if reach.hosts {
                hosts.extend(reach.live.iter().cloned());
                hosts.extend(reach.sandbox.iter().cloned());
            } else if reach.runs_written {
                hosts.push(format!(
                    "the service Jadpo runs from {}",
                    reach.runs.as_deref().unwrap_or_default()
                ));
            } else {
                hosts.push("the address its connection gives".to_string());
            }
            hosts
        })
        .unwrap_or_default();
    (hosts, sends)
}
