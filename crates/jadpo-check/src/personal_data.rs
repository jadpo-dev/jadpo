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
//! erase its copy is [`crate::provider_erasure`]'s, which reads [`sent`].

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
    /// Whether it counts only as what a provider's answer may hold, because
    /// it was sent there and the answer was sent on.
    pub answered: bool,
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
/// how it goes; what says less than another row of the same holder, sent
/// the same way, is dropped.
#[salsa::tracked(returns(ref))]
pub fn sent(db: &dyn Db, project: Project) -> Vec<Sent> {
    let left = crate::policy::flow::exits::left(db, project);
    let mut out: Vec<Sent> = Vec::new();
    // A standard type a field is of goes with the field, however each of
    // them got to the connection: an `Email` with `User.email`.
    let lookup = Lookup::new(db, project);
    let marks = ai_personal::marks(db, project);
    let of_field: Vec<(&Exit, String)> = crate::policy::flow::exits::strongest(left)
        .flat_map(|(exit, holder, _)| match holder {
            Holder::Field { host, field } => {
                ai_personal::field_holding(&lookup, marks, host, field)
                    .map(|holding| holding.of_types)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|ty| (exit, ty))
                    .collect()
            }
            Holder::Type(_) => Vec::new(),
        })
        .collect();
    for (exit, holder, how) in crate::policy::flow::exits::strongest(left) {
        if let Holder::Type(ty) = holder
            && of_field
                .iter()
                .any(|(with, held)| *with == exit && held == ty)
        {
            continue;
        }
        let sent = Sent {
            connection: exit.connection.clone(),
            holder: named(db, project, holder),
            categories: categories(db, project, holder),
            through: exit.through.clone(),
            decided: how.decided,
            answered: how.answered,
        };
        if !out.contains(&sent) {
            out.push(sent);
        }
    }
    out.sort_by(|a, b| {
        (&a.connection, &a.holder, &a.through, a.decided, a.answered).cmp(&(
            &b.connection,
            &b.holder,
            &b.through,
            b.decided,
            b.answered,
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
        Holder::Type(ty) => ai_personal::a_type(ty),
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
         Where the app's personal data goes: each connection a field marked `personal:`, or a value of a standard type that is personal data, is sent through, and the hosts that connection reaches. A value counts as sent when it may be in what goes out, may have decided it, or was sent to a provider whose answer it may be in. Each connection says whether it erases its copy when a person is erased. Written by `jadpo check`.\n\n\
         Not listed yet: what each store and cache keeps, who may read it and how long it is kept. Jadpo doesn't call a provider's `erasure:` yet.\n\n",
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
            // Said only of what is known to stay on the server: any other
            // connection with no host listed isn't said to reach none.
            let on_disk = crate::data::connections(db, project).kind(name)
                == Some(crate::data::ConnectionKind::Files);
            markdown.push_str(if on_disk {
                "Keeps its files on the server's own disk, so it reaches no host.\n\n"
            } else {
                "The network report lists no host for it, which doesn't mean it reaches none.\n\n"
            });
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
        let erasure = erasure_row(db, project, name, sends);
        markdown.push_str(&format!("{}\n\n", erasure.said));
        markdown.push_str("| Personal data | Category | Sent by | How |\n|---|---|---|---|\n");
        let mut fields = Vec::new();
        for sent in sends {
            markdown.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                shown(&sent.holder),
                category_words(&sent.categories),
                through_words(&sent.through),
                how_words(sent)
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
            entry.set("answer_only", sent.answered);
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
        connection.set("erasure", erasure.json);
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

/// How a value is in what goes, for a report.
fn how_words(sent: &Sent) -> &'static str {
    match (sent.decided, sent.answered) {
        (false, false) => "sent",
        (true, false) => "decided what was sent",
        (false, true) => "may be in a provider's answer that was sent",
        (true, true) => "may be in a provider's answer that decided what was sent",
    }
}

/// What a report says of a connection's copy.
struct ErasureRow {
    said: String,
    json: Value,
}

/// The acceptance saying why what `connection` receives stays, the last
/// written first: its file and reason.
pub fn accepted(db: &dyn Db, project: Project, connection: &str) -> Option<(String, String)> {
    crate::acceptances::files::acceptances(db, project)
        .into_iter()
        .rev()
        .find_map(|acceptance| {
            let listed = acceptance.entries.iter().any(|entry| {
                entry.subject == connection && entry.change == crate::provider_erasure::DATA_STAYS
            });
            match (listed, acceptance.reason) {
                (true, Some(reason)) => Some((acceptance.path, reason)),
                _ => None,
            }
        })
}

fn erasure_row(db: &dyn Db, project: Project, connection: &str, sends: &[&Sent]) -> ErasureRow {
    use crate::provider_erasure::{Erasure, of_connection};
    let mut json = Value::object();
    let said = match of_connection(db, project, connection, sends) {
        Erasure::Declared { call, key } => {
            json.set("status", "declared");
            json.set("call", call.as_str());
            json.set("key", key.as_str());
            format!(
                "**Erasure:** the provider's `{call}` erases its copy, with the key in `{key}`."
            )
        }
        Erasure::OwnStore => {
            json.set("status", "own_store");
            "**Erasure:** the app's own store: a file goes with the records that hold it."
                .to_string()
        }
        Erasure::Stays(stays) => {
            json.set("status", "kept");
            json.set("why", stays.said());
            match accepted(db, project, connection) {
                Some((file, reason)) => {
                    let said = format!(
                        "**Erasure:** kept after a person is erased, since {}. Accepted in `{file}`: {reason}",
                        stays.said()
                    );
                    json.set("acceptance", file);
                    json.set("reason", reason);
                    said
                }
                None => format!(
                    "**Erasure:** kept after a person is erased, since {}, and no acceptance says why.",
                    stays.said()
                ),
            }
        }
    };
    ErasureRow { said, json }
}

/// A report's JSON twin as entries to compare, by connection: each line it
/// holds, so what the approval review shows reads as the report does.
pub fn entries(json: &Value) -> BTreeMap<String, Vec<String>> {
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let mut out = BTreeMap::new();
    let connections = json
        .get("connections")
        .and_then(Value::as_array)
        .unwrap_or_default();
    for connection in connections {
        let mut lines = Vec::new();
        if let Some(erasure) = connection.get("erasure") {
            lines.push(match text(erasure, "status").as_str() {
                "declared" => format!(
                    "erased by `{}`, with the key in `{}`",
                    text(erasure, "call"),
                    text(erasure, "key")
                ),
                "own_store" => "the app's own store".to_string(),
                _ => match erasure.get("reason").and_then(Value::as_str) {
                    Some(reason) => format!("kept, accepted: {reason}"),
                    None => format!("kept: {}", text(erasure, "why")),
                },
            });
        }
        let hosts = connection
            .get("hosts")
            .and_then(Value::as_array)
            .unwrap_or_default();
        for host in hosts {
            let named = ["address", "setting", "service"]
                .iter()
                .find_map(|key| host.get(key).and_then(Value::as_str));
            if let Some(named) = named {
                lines.push(format!("host {named}"));
            }
        }
        let rows = connection
            .get("sent")
            .and_then(Value::as_array)
            .unwrap_or_default();
        for row in rows {
            let flag = |key: &str| row.get(key) == Some(&Value::Bool(true));
            let holder = match row.get("type").and_then(Value::as_str) {
                Some(ty) => ai_personal::a_type(ty),
                None => format!("`{}.{}`", text(row, "host"), text(row, "field")),
            };
            let by = match row.get("name").and_then(Value::as_str) {
                Some(name) => format!("{} `{name}`", text(row, "through")),
                None => text(row, "through"),
            };
            let categories: Vec<String> = row
                .get("categories")
                .and_then(Value::as_array)
                .unwrap_or_default()
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            let mut categories = categories;
            if flag("sensitive") {
                categories.push("sensitive".to_string());
            }
            let how = match (flag("decided_only"), flag("answer_only")) {
                (false, false) => "sent",
                (true, false) => "decided what was sent",
                (false, true) => "may be in a provider's answer that was sent",
                (true, true) => "may be in a provider's answer that decided what was sent",
            };
            lines.push(format!("{holder} ({}): {by}, {how}", categories.join(", ")));
        }
        lines.sort();
        out.insert(
            format!("connection {}", text(connection, "connection")),
            lines,
        );
    }
    out
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
/// hosts it sends them to: `jadpo package check`'s section, and a release's
/// facts ([`crate::release::facts`]).
pub fn package_sends(
    db: &dyn Db,
    project: Project,
    folder: &str,
) -> (Vec<String>, Vec<PackageSend>) {
    let left =
        crate::policy::flow::exits::package_left(db, project, Name::new(db, folder.to_string()));
    let sends = crate::policy::flow::exits::strongest(left)
        .filter_map(|(exit, holder, how)| match &exit.through {
            Through::Operation(operation) => Some(PackageSend {
                operation: operation.clone(),
                holder: named(db, project, holder),
                categories: categories(db, project, holder),
                decided: how.decided,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn twin(sensitive: bool, answered: bool) -> Value {
        let mut row = Value::object();
        row.set("host", "User");
        row.set("field", "notes");
        row.set("categories", vec!["health".to_string()]);
        row.set("sensitive", sensitive);
        row.set("through", "operation");
        row.set("name", "customers.create");
        row.set("decided_only", false);
        row.set("answer_only", answered);
        let mut erasure = Value::object();
        erasure.set("status", "kept");
        erasure.set("why", "`AcmePay.v1` declares no `erasure:`");
        let mut connection = Value::object();
        connection.set("connection", "pay");
        connection.set("hosts", Vec::<Value>::new());
        connection.set("erasure", erasure);
        connection.set("sent", vec![row]);
        let mut json = Value::object();
        json.set("connections", vec![connection]);
        json
    }

    /// The approval review flags a sensitive category, and says what is only
    /// what a provider's answer may hold.
    #[test]
    fn what_the_review_compares_says_what_the_report_says() {
        assert_eq!(
            entries(&twin(true, false))["connection pay"],
            [
                "`User.notes` (health, sensitive): operation `customers.create`, sent",
                "kept: `AcmePay.v1` declares no `erasure:`",
            ]
        );
        assert_eq!(
            entries(&twin(false, true))["connection pay"],
            [
                "`User.notes` (health): operation `customers.create`, may be in a provider's answer that was sent",
                "kept: `AcmePay.v1` declares no `erasure:`",
            ]
        );
    }
}
