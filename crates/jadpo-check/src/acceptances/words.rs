//! Each change that needs a person, said in plain words (area 17,
//! "Reviewing a change"): who can now do what to which data, as the
//! approval report and `jadpo approval` say it. Worked out where the
//! change is read from its record, so the words carry what the record
//! says, such as which callers a widening lets in.

use jadpo_graph::migration::{Removal, RemovalKind};

use super::compare::Family;

/// How a caller a grant list names reads in a sentence: `Access.public`
/// is anyone, a role is written as it is, and a field of the record
/// (`borrower_id`) is whoever it names.
pub fn caller(who: &str) -> String {
    match who {
        "Access.public" => "anyone".to_string(),
        "Access.authenticated" => "anyone signed in".to_string(),
        "Access.system" => "schedules and subscribers".to_string(),
        who if who.contains('.') => format!("`{who}`"),
        field => format!("whoever `{field}` names"),
    }
}

/// Several callers in a sentence: `a`, `a and b`, `a, b and c`.
pub fn callers(who: &[String]) -> String {
    let words: Vec<String> = who.iter().map(|who| caller(who)).collect();
    match words.as_slice() {
        [] => "nobody".to_string(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// `main GET /books` from its quoted subject: the API and the route.
fn route(subject: &str) -> (String, String) {
    let text = subject.trim_matches('"');
    match text.split_once(' ') {
        Some((api, route)) => (api.to_string(), route.to_string()),
        None => (String::new(), text.to_string()),
    }
}

/// What a grant's effect lets a caller do, as a verb on the records.
fn verb(effect: &str) -> &str {
    match effect {
        "create" => "create",
        "read" => "read",
        "update" => "change",
        "delete" => "delete",
        other => other,
    }
}

/// A listed action's or query's line that lets `who` in.
pub fn can_run(subject: &str, who: &[String]) -> String {
    format!("{} can now run `{subject}`", callers(who))
}

/// A widening of a grant, `lending.Book.read` or `lending.Book.title.read`,
/// that lets `who` in.
pub fn widened(subject: &str, who: &[String]) -> String {
    let callers = callers(who);
    let Some((what, effect)) = subject.rsplit_once('.') else {
        return can_run(subject, who);
    };
    // `lending.Book` is an entity's records; `lending.Book.title` a field.
    let field = what.split('.').count() > 2;
    if field {
        format!("{callers} can now {} `{what}`", verb(effect))
    } else {
        format!("{callers} can now {} `{what}` records", verb(effect))
    }
}

/// The plain words of a change read from a record, when the record says
/// no more than its subject and kind.
pub fn said(family: &Family, subject: &str, change: &str) -> String {
    let events = *family == Family::Events;
    match change {
        "ApiChange.route_removed" => {
            let (api, route) = route(subject);
            format!("`{route}` is gone from API `{api}`")
        }
        "ApiChange.input_field_required" if events => {
            format!("every event must now carry `{subject}`, those already waiting included")
        }
        "ApiChange.input_field_required" => format!("clients must now send `{subject}`"),
        "ApiChange.rule_tightened" if events => {
            format!("`{subject}` now allows fewer values than events already waiting may hold")
        }
        "ApiChange.rule_tightened" => {
            format!("`{subject}` now refuses some values clients send")
        }
        "ApiChange.output_field_removed" if events => {
            format!("subscribers no longer get `{subject}`")
        }
        "ApiChange.output_field_removed" => format!("clients no longer get `{subject}`"),
        "ApiChange.output_field_retyped" if events => {
            format!("subscribers can now get other values in `{subject}`")
        }
        "ApiChange.output_field_retyped" => {
            format!("clients can now get other values in `{subject}`")
        }
        "ApiChange.failure_changed" => {
            format!("failure `{subject}` is gone, or answers with another status")
        }
        "ApiChange.event_removed" => format!("event `{subject}` is gone"),
        "PolicyChange.route_public" => {
            let (api, route) = route(subject);
            if api == "main" || api.is_empty() {
                format!("anyone can now call `{route}`, without signing in")
            } else {
                format!("anyone can now call `{route}` of API `{api}`, without signing in")
            }
        }
        "PolicyChange.confirmation_removed" => {
            let (api, route) = route(subject);
            if api == "main" || api.is_empty() {
                format!(
                    "a signed-in session alone can now run `{route}`, without the person's passkey"
                )
            } else {
                format!(
                    "a signed-in session alone can now run `{route}` of API `{api}`, without the person's passkey"
                )
            }
        }
        "PolicyChange.restriction_removed" => format!(
            "`{subject}` no longer has a restriction of its own, so it follows its record's grants"
        ),
        "PolicyChange.tenancy_loosened" if subject == "tenant" => {
            "records are no longer kept to their tenant".to_string()
        }
        "PolicyChange.tenancy_loosened" => {
            format!("`{subject}` records are now shared by every tenant")
        }
        "PolicyChange.shared_entity_added" => {
            format!("new entity `{subject}` is shared by every tenant")
        }
        "PolicyChange.listing_removed" => format!(
            "`{subject}` is no longer listed under `actions:`, so its entities' grants decide who can run it"
        ),
        "NetworkChange.host_added" => {
            format!("connection `{subject}` can now reach a host it couldn't")
        }
        "PackageChange.git_source" => format!(
            "`{subject}` is installed from git, where no registry verifies who published it"
        ),
        "DomainChange.created" => format!("new domain `{subject}`"),
        "ThresholdChange.raised" => {
            format!("threshold `{subject}` is raised, so its alert goes off later")
        }
        "ThresholdChange.removed" => {
            format!("threshold `{subject}` is gone, so nothing alerts on it")
        }
        "ThresholdChange.lowered" => format!(
            "`{subject}` is lowered, so the disk fills further before writes are refused and its alert goes off"
        ),
        change => format!("`{subject}`: `{change}`"),
    }
}

/// Names in backticks in a sentence: `a`, `a` and `b`, `a`, `b` and `c`.
fn names(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Exactly what a migration's removal takes (area 5, "Migrations"; area
/// 18, `DataChange`): the store, the table and each column, and what
/// happens to the files its rows hold and to the entries of its history.
pub fn removal(removal: &Removal) -> String {
    let subject = removal.subject();
    let (store, table) = (&removal.store, &removal.table);
    let columns: Vec<String> = removal.columns.iter().map(|(n, _)| n.clone()).collect();
    let mut said = match &removal.kind {
        RemovalKind::ColumnDropped => {
            let held: Vec<String> = removal
                .columns
                .iter()
                .map(|(column, ty)| format!("`{table}.{column}` (`{ty}`)"))
                .collect();
            let noun = if held.len() == 1 { "column" } else { "columns" };
            let list = match held.as_slice() {
                [one] => one.clone(),
                [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
                [] => String::new(),
            };
            format!(
                "`{subject}` is dropped for good from store `{store}`: {noun} {list} and every value in it"
            )
        }
        RemovalKind::TableDropped => format!(
            "`{subject}` is dropped for good from store `{store}`: table `{table}`, every record in it and its columns {}",
            names(&columns)
        ),
        RemovalKind::EntityRemoved => format!(
            "`{subject}` leaves the app: table `{table}` and its records stay in store `{store}`, retired, where erasure and retention no longer reach them, until a later migration drops them"
        ),
        RemovalKind::EntityMoved { to } => format!(
            "`{subject}` moves from store `{store}` to `{to}`: table `{table}` starts empty there, and its records stay behind, retired, since nothing copies them"
        ),
    };
    let dropped = matches!(
        removal.kind,
        RemovalKind::ColumnDropped | RemovalKind::TableDropped
    );
    if dropped && removal.files {
        said.push_str(
            ", letting go of the files they hold, whose bytes go once no record holds them and a `File` field still names their store",
        );
    }
    if removal.history {
        said.push_str(match removal.kind {
            RemovalKind::ColumnDropped => {
                "; the values `jadpo_audit` entries hold of it stay until their retention ends"
            }
            _ => "; any `jadpo_audit` entries of its records stay, with nothing left to purge them",
        });
    }
    said
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_no_longer_confirmed_says_what_is_enough_now() {
        assert_eq!(
            said(
                &Family::Api,
                "\"main POST /todos\"",
                "PolicyChange.confirmation_removed"
            ),
            "a signed-in session alone can now run `POST /todos`, without the person's passkey"
        );
    }

    #[test]
    fn callers_read_as_people() {
        assert_eq!(caller("Access.public"), "anyone");
        assert_eq!(caller("StaffRole.admin"), "`StaffRole.admin`");
        assert_eq!(caller("borrower_id"), "whoever `borrower_id` names");
        assert_eq!(
            callers(&["Access.authenticated".into(), "StaffRole.admin".into()]),
            "anyone signed in and `StaffRole.admin`"
        );
    }

    #[test]
    fn a_loosened_threshold_says_what_stops_alerting() {
        assert_eq!(
            said(
                &Family::Thresholds,
                "hello.greeting_route.p95",
                "ThresholdChange.raised"
            ),
            "threshold `hello.greeting_route.p95` is raised, so its alert goes off later"
        );
        assert_eq!(
            said(
                &Family::Thresholds,
                "hello.greeting",
                "ThresholdChange.removed"
            ),
            "threshold `hello.greeting` is gone, so nothing alerts on it"
        );
        assert_eq!(
            said(
                &Family::Thresholds,
                "uploads.free_space_floor",
                "ThresholdChange.lowered"
            ),
            "`uploads.free_space_floor` is lowered, so the disk fills further before writes are refused and its alert goes off"
        );
    }

    #[test]
    fn a_widening_says_who_can_now_do_what() {
        assert_eq!(
            widened("work.Project.budget.read", &["CompanyRole.viewer".into()]),
            "`CompanyRole.viewer` can now read `work.Project.budget`"
        );
        assert_eq!(
            widened("lending.Book.read", &["Access.public".into()]),
            "anyone can now read `lending.Book` records"
        );
        assert_eq!(
            can_run("lending.borrow", &["StaffRole.clerk".into()]),
            "`StaffRole.clerk` can now run `lending.borrow`"
        );
    }
}
