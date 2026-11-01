//! The sections of the root `application { }` (area 1, "What
//! `application { }` holds"): one list of every section an area page
//! decides. Anything else is `semantic.application_section_unknown`, with
//! the edit to the section a near-miss spelling meant, so a setting meant
//! to protect the app is never dropped without a word; a section decided
//! and not built is `toolchain.not_built_yet`, never unknown.

use jadpo_db::{Database, Layout};

use crate::support;

const UNKNOWN: &str = "semantic.application_section_unknown";

/// Every problem of a project whose `app.jadpo` is `app`.
fn problems(app: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("app.jadpo", app)])
}

#[test]
fn a_misspelt_section_is_refused_with_the_edit_to_the_one_it_meant() {
    let text = "application {\n    revocation_dela: 5m\n}\n";
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("app.jadpo", text)]);
    let found = jadpo_check::check(&db, project);
    assert_eq!(found.len(), 1, "{found:?}");
    let problem = &found[0];
    assert_eq!(problem.rule.id, UNKNOWN);
    assert_eq!(
        problem.message,
        "`revocation_dela` isn't something `application { }` holds"
    );
    let next = problem.next.as_ref().expect("a next step");
    assert_eq!(
        next.text,
        "Use `revocation_delay` instead of `revocation_dela`"
    );
    let edit = &next.edits[0];
    let mut fixed = text.to_string();
    fixed.replace_range(
        edit.span.start as usize..edit.span.end as usize,
        &edit.replacement,
    );
    assert_eq!(fixed, "application {\n    revocation_delay: 5m\n}\n");
    assert_eq!(problems(&fixed), Vec::<String>::new());
}

#[test]
fn a_section_nothing_is_near_is_refused_without_an_edit() {
    let text = "application {\n    colour_scheme: \"dark\"\n}\n";
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("app.jadpo", text)]);
    let found = jadpo_check::check(&db, project);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, UNKNOWN);
    let next = found[0].next.as_ref().expect("a next step");
    assert_eq!(
        next.text,
        "Remove `colour_scheme` from `application { }`, or move it where it belongs"
    );
    assert!(next.edits.is_empty(), "{:?}", next.edits);
}

#[test]
fn an_old_spelling_is_refused_with_the_section_it_is_now() {
    let found =
        problems("application {\n    mail: {\n        from: \"desk@example.com\"\n    }\n}\n");
    assert_eq!(
        found,
        [
            "app.jadpo semantic.application_section_unknown: `mail` isn't something `application { }` holds"
        ]
    );
}

#[test]
fn a_dotted_section_is_refused_as_written() {
    let found = problems(
        "application {\n    connections.main: {\n        driver: Driver.postgres\n    }\n}\n",
    );
    assert_eq!(
        found,
        [
            "app.jadpo semantic.application_section_unknown: `connections.main` isn't something `application { }` holds"
        ]
    );
}

#[test]
fn a_bare_name_is_not_a_section() {
    let found = problems("application {\n    display_name\n}\n");
    assert!(found.len() == 1 && found[0].contains(UNKNOWN), "{found:?}");
}

#[test]
fn a_rule_is_refused_once_where_rules_are() {
    assert_eq!(
        support::rules(
            Layout::Loose,
            &[("app.jadpo", "application {\n    rule safe: true\n}\n")]
        ),
        ["type.rule_outside_object"]
    );
}

#[test]
fn every_section_an_area_page_decides_is_known() {
    // Each built section, written with a value its own check may refuse:
    // whatever that check says, the section is never unknown.
    for section in jadpo_check::application::SECTIONS {
        let found = problems(&format!("application {{\n    {section}: {{}}\n}}\n"));
        assert!(
            found.iter().all(|problem| !problem.contains(UNKNOWN)),
            "{section}: {found:?}"
        );
    }
}

#[test]
fn a_section_decided_and_not_built_is_not_built_yet_rather_than_unknown() {
    for (section, _) in jadpo_check::application::NOT_BUILT {
        let found = problems(&format!("application {{\n    {section}: {{}}\n}}\n"));
        assert_eq!(found.len(), 1, "{section}: {found:?}");
        assert!(
            found[0].starts_with("app.jadpo toolchain.not_built_yet:")
                && found[0].contains(&format!("`{section}:`")),
            "{section}: {found:?}"
        );
    }
}

#[test]
fn the_list_is_every_section_the_area_pages_give() {
    let mut all: Vec<&str> = jadpo_check::application::SECTIONS.to_vec();
    all.extend(jadpo_check::application::NOT_BUILT.iter().map(|(s, _)| *s));
    all.sort_unstable();
    assert_eq!(
        all,
        [
            "ai",
            "apis",
            "bot_check",
            "client_address",
            "connections",
            "deployables",
            "display_name",
            "documents",
            "email",
            "events",
            "flags",
            "grants",
            "jadpo",
            "packages",
            "privacy",
            "publishing",
            "rate_limits",
            "receiving",
            "revocation_delay",
            "scanning",
            "schedules",
            "steps",
            "telemetry",
            "translations",
        ]
    );
}
