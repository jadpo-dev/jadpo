//! The Jadpo release line an application is written for (area 1, "The
//! root"; area 14, "Which jadpo runs"; roadmap chunks D3 and D14):
//! `application { jadpo: "0.1" }`, the line as its major and minor
//! numbers, and a project on another line refused with that error alone:
//! `toolchain.version_newer` for a later line, `toolchain.version_older`
//! for an earlier one.

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
fn this_line_checks() {
    let text = app(&format!("    jadpo: \"{}\"\n", this_version()));
    assert_eq!(project(&text, &[]), Vec::<String>::new());
}

#[test]
fn leaving_it_out_checks_nothing() {
    assert_eq!(project("application {}\n", &[]), Vec::<String>::new());
}

#[test]
fn a_later_major_or_minor_line_is_refused() {
    let (major, minor) = numbers();
    for version in [format!("{}.0", major + 1), format!("{major}.{}", minor + 1)] {
        let text = app(&format!("    jadpo: \"{version}\"\n"));
        assert_eq!(
            project(&text, &[]),
            vec![format!(
                "app.jadpo toolchain.version_newer: This project needs Jadpo {version}"
            )],
        );
    }
}

#[test]
fn an_earlier_line_is_refused_too() {
    // A line, not a minimum: a 0.x minor release may break an app, so a
    // later jadpo doesn't compile it silently.
    let (major, minor) = numbers();
    let mut earlier = vec!["0.0".to_string()];
    if minor > 0 {
        earlier.push(format!("{major}.{}", minor - 1));
    }
    for version in earlier {
        if version == this_version() {
            continue;
        }
        let text = app(&format!("    jadpo: \"{version}\"\n"));
        assert_eq!(
            project(&text, &[]),
            vec![format!(
                "app.jadpo toolchain.version_older: This project needs Jadpo {version}"
            )],
            "{version}",
        );
    }
}

#[test]
fn a_project_on_an_earlier_line_reports_nothing_else() {
    let text = app("    jadpo: \"0.0\"\n    revocation_delay: soon\n");
    assert_eq!(
        project(&text, &[]),
        vec!["app.jadpo toolchain.version_older: This project needs Jadpo 0.0"],
    );
}

#[test]
fn a_lock_pin_on_another_line_is_out_of_date() {
    let text = app(&format!("    jadpo: \"{}\"\n", this_version()));
    let with_lock = |lock: &str| {
        let db = jadpo_db::Database::default();
        let project = support::project_with_records(
            &db,
            Layout::Project,
            &[("app.jadpo", text.as_str())],
            &[("jadpo.lock", lock)],
        );
        jadpo_check::check(&db, project)
            .iter()
            .map(|d| {
                let path = d.primary.as_ref().map_or("", |l| l.span.path.as_str());
                format!("{path} {}: {}", d.rule.id, d.message)
            })
            .collect::<Vec<String>>()
    };
    let on_line = jadpo_db::write_pinned_lock(Some(&format!("{}.7", this_version())), &[]);
    assert_eq!(with_lock(&on_line), Vec::<String>::new());
    let off_line = jadpo_db::write_pinned_lock(Some("99.0.1"), &[]);
    assert_eq!(
        with_lock(&off_line),
        vec![format!(
            "jadpo.lock record.out_of_date: `jadpo.lock` pins Jadpo 99.0.1, which isn't on line {}",
            this_version()
        )],
    );
}

/// This Jadpo's major and minor numbers.
fn numbers() -> (u64, u64) {
    this_version()
        .split_once('.')
        .map(|(a, b)| (a.parse::<u64>().unwrap(), b.parse::<u64>().unwrap()))
        .unwrap()
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
        vec!["app.jadpo toolchain.version_newer: This project needs Jadpo 99.0"],
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
        vec!["main.jadpo toolchain.version_newer: This project needs Jadpo 99.0"],
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
