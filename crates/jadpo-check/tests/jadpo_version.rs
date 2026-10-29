//! The Jadpo version an application needs (area 1, "The root"; area 14,
//! "Scaffold"; roadmap chunk D3): `application { jadpo: "0.1" }`, the
//! earliest version as its major and minor numbers, and a project needing a
//! newer Jadpo refused with that error alone.

use jadpo_db::Layout;

use crate::support;

fn app(settings: &str) -> String {
    format!("application {{\n{settings}}}\n")
}

/// This Jadpo's major and minor numbers, as `jadpo:` writes them.
fn this_version() -> String {
    let mut parts = env!("CARGO_PKG_VERSION").split('.');
    format!("{}.{}", parts.next().unwrap(), parts.next().unwrap())
}

fn project(app_text: &str, other: &[(&str, &str)]) -> Vec<String> {
    let mut files = vec![("app.jadpo", app_text)];
    files.extend_from_slice(other);
    support::problems(Layout::Project, &files)
}

#[test]
fn this_version_and_older_ones_check() {
    let current = this_version();
    for version in [current.as_str(), "0.0"] {
        let text = app(&format!("    jadpo: \"{version}\"\n"));
        assert_eq!(project(&text, &[]), Vec::<String>::new(), "{version}");
    }
}

#[test]
fn leaving_it_out_checks_nothing() {
    assert_eq!(project("application {}\n", &[]), Vec::<String>::new());
}

#[test]
fn a_newer_major_or_minor_version_is_refused() {
    let (major, minor) = this_version()
        .split_once('.')
        .map(|(a, b)| (a.parse::<u64>().unwrap(), b.parse::<u64>().unwrap()))
        .unwrap();
    for version in [format!("{}.0", major + 1), format!("{major}.{}", minor + 1)] {
        let text = app(&format!("    jadpo: \"{version}\"\n"));
        assert_eq!(
            project(&text, &[]),
            vec![format!(
                "app.jadpo toolchain.version_newer: This project needs Jadpo {version} or later"
            )],
        );
    }
}

#[test]
fn a_project_needing_a_newer_jadpo_reports_nothing_else() {
    // What a later Jadpo added would only read as mistakes here: a name
    // this one doesn't know, and syntax it doesn't parse.
    let text = app("    jadpo: \"99.0\"\n    revocation_delay: soon\n");
    let found = project(
        &text,
        &[
            (
                "domains/planning/types/note.jadpo",
                "type Note = Text {\n    max_length: missing\n}\n",
            ),
            (
                "domains/planning/types/later.jadpo",
                "type Later = Text {\n    max_length: 3 +\n}\n",
            ),
        ],
    );
    assert_eq!(
        found,
        vec!["app.jadpo toolchain.version_newer: This project needs Jadpo 99.0 or later"],
    );
}

#[test]
fn a_loose_file_is_held_to_it_too() {
    let found = support::problems(
        Layout::Loose,
        &[("main.jadpo", "application {\n    jadpo: \"99.0\"\n}\n")],
    );
    assert_eq!(
        found,
        vec!["main.jadpo toolchain.version_newer: This project needs Jadpo 99.0 or later"],
    );
}

#[test]
fn anything_but_a_major_and_minor_number_as_text_is_invalid() {
    for written in [
        "0.1",
        "\"1\"",
        "\"0.1.0\"",
        "\"01.0\"",
        "\"v0.1\"",
        "\"\"",
        "\"0.x\"",
    ] {
        let text = app(&format!("    jadpo: {written}\n"));
        assert_eq!(
            project(&text, &[]),
            vec![format!(
                "app.jadpo toolchain.version_invalid: `{written}` isn't a Jadpo version"
            )],
            "{written}",
        );
    }
}
