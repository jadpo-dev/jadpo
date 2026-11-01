//! Protections the area pages promise that aren't built yet are
//! `toolchain.not_built_yet` where they are written (each area's "What is
//! built"), never accepted and then ignored, nor reported as wrong syntax:
//! area 15's event `retention:`, and the application's and a route's
//! `bot_check:`. Area 15's `person: true` is built (chunk P1,
//! [`crate::persons`]), and so are `retention:` on an entity and its
//! fields (chunk P2, [`crate::retention`]), a reference's `erase:` (chunk
//! P3, [`crate::erasure`]) and area 16's audit trail (chunk B11,
//! [`crate::audit`]).

use jadpo_db::Layout;

use crate::support;

/// The problems of `source` alone, as `rule: message`.
fn problems(source: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("main.jadpo", source)])
        .into_iter()
        .map(|problem| problem.trim_start().to_string())
        .collect()
}

/// Whether the one problem is `toolchain.not_built_yet`, naming `what`.
fn refused_once(source: &str, what: &str) {
    let found = problems(source);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("main.jadpo toolchain.not_built_yet:") && found[0].contains(what),
        "{found:?}"
    );
}

#[test]
fn an_events_retention_is_refused() {
    refused_once(
        r#"type VisitorArrived = Event {
    ip: IpAddress? {
        retention: {
            after: 30.days
        }
    }
}
"#,
        "`retention:`",
    );
}

#[test]
fn a_routes_bot_check_is_not_built_yet_rather_than_unknown() {
    refused_once(
        r#"action ping() -> Unit {
    assert true
}

routes Pings {
    prefix: /pings

    POST / {
        authentication: RouteAuthentication.none
        status: SuccessStatus.no_content
        run: ping()
        bot_check: true
    }
}
"#,
        "`bot_check:`",
    );
}

#[test]
fn a_field_named_like_an_unbuilt_section_is_a_field() {
    let found = problems(
        r#"type Amount = Int {}

entity Ledger {
    id: Uuid
    audit: Amount?
    retention: Amount?
    person: Amount?
}
"#,
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_misspelt_section_is_still_unknown() {
    let found = problems(
        r#"entity Invoice {
    id: Uuid
    audits: {
        retention: {
            after: 7.days
        }
    }
}
"#,
    );
    assert!(
        found
            .iter()
            .all(|problem| !problem.contains("toolchain.not_built_yet")),
        "{found:?}"
    );
}

#[test]
fn an_applications_bot_check_is_not_built_yet() {
    let found = support::problems(
        Layout::Loose,
        &[(
            "app.jadpo",
            "application {\n    bot_check: {\n        connection: turnstile\n    }\n}\n",
        )],
    );
    assert!(
        found.len() == 1 && found[0].contains("toolchain.not_built_yet:"),
        "{found:?}"
    );
}
