//! Your own company's repositories (area 11, "Installing safely"):
//! `packages:`'s `own_sources:`. Each entry is a host and a path ending in
//! `/`; a repository is under one only by its host and whole path
//! segments, so no lookalike address gets in without a person.

use jadpo_check::own_sources::{covering, covers};
use jadpo_db::{Database, Layout};

use crate::support;

const RULE: &str = "package.own_source_invalid";

fn own(entries: &[&str]) -> Vec<String> {
    entries.iter().map(|entry| entry.to_string()).collect()
}

/// The rules and messages a project whose `app.jadpo` is `app` gets.
fn problems(app: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("app.jadpo", app)])
}

fn section(body: &str) -> String {
    format!("application {{\n    packages: {{\n        {body}\n    }}\n}}\n")
}

#[test]
fn a_repository_under_an_entry_is_covered_however_git_reads_it() {
    let entries = own(&["github.com/acme/", "git.acme.example/platform/"]);
    for repository in [
        "github.com/acme/postcodes",
        "github.com/acme/postcodes.git",
        "https://github.com/acme/postcodes",
        "https://github.com/acme/postcodes.git",
        "ssh://git@github.com/acme/postcodes.git",
        "ssh://github.com/acme/postcodes",
        "git@github.com:acme/postcodes.git",
        // A host's name has no case; a repository's path may.
        "GitHub.com/acme/postcodes",
        "https://GITHUB.COM/acme/postcodes",
        // Below the entry, however deep.
        "github.com/acme/tools/postcodes",
        "git.acme.example/platform/postcodes",
    ] {
        assert!(covering(&entries, repository).is_some(), "{repository}");
    }
    assert_eq!(
        covering(&entries, "git.acme.example/platform/postcodes"),
        Some("git.acme.example/platform/")
    );
}

/// What must never count as under `github.com/acme/`: each is someone
/// else's, or can't be told apart from someone else's.
#[test]
fn a_lookalike_address_is_under_no_entry() {
    let entries = own(&["github.com/acme/"]);
    for repository in [
        // Prefixes of a name.
        "github.com/acme-evil/postcodes",
        "github.com/acmeinc/postcodes",
        "github.com/acme.evil/postcodes",
        "github.com/acme.git/postcodes",
        "github.com/acme",
        "github.com/acme/",
        "github.com/acme.git",
        "github.com/acme/.git",
        "github.com.evil.example/acme/postcodes",
        "evilgithub.com/acme/postcodes",
        "evil.example/github.com/acme/postcodes",
        "https://evil.example/github.com/acme/postcodes",
        // A path's case: another owner on a host that tells them apart.
        "github.com/ACME/postcodes",
        "github.com/Acme/postcodes",
        // The account named before the host, and the host named as one.
        "github.com@evil.example/acme/postcodes",
        "https://github.com@evil.example/acme/postcodes",
        "https://github.com:x@evil.example/acme/postcodes",
        "ssh://git@evil.example/github.com/acme/postcodes",
        "ssh://github.com@evil.example/acme/postcodes",
        "git@evil.example:github.com/acme/postcodes",
        // An account git would sign in as, which a lock never needs.
        "https://token@github.com/acme/postcodes",
        "https://user:secret@github.com/acme/postcodes",
        "ssh://root@github.com/acme/postcodes",
        // Another server at the same name.
        "github.com:8443/acme/postcodes",
        "https://github.com:8443/acme/postcodes",
        "ssh://git@github.com:2222/acme/postcodes",
        "git@github.com:2222/acme/postcodes",
        "github.com:acme/postcodes",
        // Walking out of the folder.
        "github.com/acme/../evil/postcodes",
        "https://github.com/acme/../evil/postcodes",
        "github.com/acme/./postcodes",
        "github.com/acme//postcodes",
        "github.com/acme/%2e%2e/evil/postcodes",
        "github.com/acme/..%2fevil/postcodes",
        "github.com/acme%2fpostcodes",
        "github.com/acme/..\\evil/postcodes",
        "github.com\\acme\\postcodes",
        // What follows the path, and what ends it early.
        "github.com/acme/postcodes/",
        "evil.example/x?github.com/acme/postcodes",
        "evil.example/x#github.com/acme/postcodes",
        "github.com/acme/postcodes?x=1",
        "github.com/acme/postcodes#x",
        // Schemes that authenticate nothing, or read this machine.
        "http://github.com/acme/postcodes",
        "git://github.com/acme/postcodes",
        "file://github.com/acme/postcodes",
        "file:///github.com/acme/postcodes",
        "HTTPS://github.com/acme/postcodes",
        "ext::sh -c evil github.com/acme/postcodes",
        // Hosts that only read the same.
        "github.com./acme/postcodes",
        ".github.com/acme/postcodes",
        "gіthub.com/acme/postcodes",
        // Folders on this machine.
        "./github.com/acme/postcodes",
        "../github.com/acme/postcodes",
        "/github.com/acme/postcodes",
        "",
        " github.com/acme/postcodes",
        "github.com/acme/post codes",
    ] {
        assert_eq!(covering(&entries, repository), None, "{repository:?}");
    }
}

#[test]
fn a_port_is_part_of_the_server_an_entry_names() {
    let entries = own(&["git.acme.example:8443/platform/"]);
    assert!(covering(&entries, "git.acme.example:8443/platform/postcodes").is_some());
    assert!(covering(&entries, "https://git.acme.example:8443/platform/postcodes").is_some());
    assert!(covering(&entries, "git.acme.example/platform/postcodes").is_none());
    assert!(covering(&entries, "git.acme.example:443/platform/postcodes").is_none());
    assert!(covering(&entries, "git.acme.example:08443/platform/postcodes").is_none());
}

/// A new entry widens trust unless an entry there before covers all of it.
#[test]
fn an_entry_covers_what_is_below_it() {
    let was = own(&["github.com/acme/tools/"]);
    assert!(covers(&was, "github.com/acme/tools/"));
    assert!(covers(&was, "github.com/acme/tools/deep/"));
    assert!(!covers(&was, "github.com/acme/"));
    assert!(!covers(&was, "github.com/acme/tools-evil/"));
    assert!(!covers(&was, "github.com/ACME/tools/"));
    assert!(!covers(&was, "github.com:8443/acme/tools/"));
    assert!(!covers(&own(&[]), "github.com/acme/"));
}

#[test]
fn a_good_section_reads_cleanly() {
    assert_eq!(
        problems(&section(
            "own_sources: [\"github.com/acme/\", \"git.acme.example/platform/\", \"git.acme.example:8443/tools/\"]"
        )),
        Vec::<String>::new()
    );
    assert_eq!(problems(&section("own_sources: []")), Vec::<String>::new());
}

#[test]
fn an_entry_is_refused_unless_it_is_a_host_and_a_path_ending_in_a_slash() {
    for (entry, says) in [
        ("https://github.com/acme/", "scheme"),
        ("ssh://git@github.com/acme/", "scheme"),
        ("git@github.com:acme/", "account"),
        ("user@github.com/acme/", "account"),
        ("github.com/acme", "ends in `/`"),
        ("github.com/", "every repository"),
        ("github.com", "ends in `/`"),
        ("GitHub.com/acme/", "lower case"),
        ("github.com:port/acme/", "host"),
        ("github.com:0/acme/", "host"),
        ("github.com:99999/acme/", "host"),
        ("github.com:0443/acme/", "host"),
        ("acme/tools/", "host"),
        ("/acme/tools/", "host"),
        ("github.com./acme/", "host"),
        ("gіthub.com/acme/", "host"),
        ("github.com//acme/", "path"),
        ("github.com/acme//", "path"),
        ("github.com/../acme/", "path"),
        ("github.com/acme/./", "path"),
        ("github.com/acme%2f/", "path"),
        ("github.com/acme?x/", "path"),
        ("github.com/acme#x/", "path"),
        ("github.com/ac me/", "path"),
        ("github.com/acme\\\\x/", "path"),
        ("github.com/acme/postcodes.git/", "one repository"),
        ("", "ends in `/`"),
    ] {
        let found = problems(&section(&format!("own_sources: [\"{entry}\"]")));
        assert_eq!(found.len(), 1, "{entry}: {found:?}");
        assert!(found[0].contains(RULE), "{entry}: {found:?}");
        let db = Database::default();
        let text = section(&format!("own_sources: [\"{entry}\"]"));
        let project = support::project(&db, Layout::Loose, &[("app.jadpo", &text)]);
        let problem = &jadpo_check::check(&db, project)[0];
        let reason = problem.reason.clone().unwrap_or_default();
        assert!(reason.contains(says), "{entry}: {reason}");
        // An entry refused covers nothing.
        assert_eq!(
            jadpo_check::own_sources::own_sources(&db, project)
                .entries
                .len(),
            0,
            "{entry}"
        );
    }
}

#[test]
fn a_section_holds_only_a_list_of_text_written_once() {
    for body in [
        "own_sources: \"github.com/acme/\"",
        "own_sources: [acme]",
        "own_sources: [\"github.com/acme/\", \"github.com/acme/\"]",
        "own_source: [\"github.com/acme/\"]",
        "own_sources: [\"github.com/acme/\"]\n        own_sources: [\"github.com/other/\"]",
    ] {
        let found = problems(&section(body));
        assert!(
            found.len() == 1 && found[0].contains(RULE),
            "{body}: {found:?}"
        );
    }
    let found = problems("application {\n    packages: \"github.com/acme/\"\n}\n");
    assert!(found.len() == 1 && found[0].contains(RULE), "{found:?}");
    let found = problems(
        "application {\n    packages: {\n        own_sources: []\n    }\n    packages: {\n        own_sources: []\n    }\n}\n",
    );
    assert!(found.len() == 1 && found[0].contains(RULE), "{found:?}");
}

/// A near miss of the one setting is respelt, as `application { }`'s own
/// sections are: a misspelt list would otherwise let nothing in, and a
/// person looking for it would see no error.
#[test]
fn a_misspelt_setting_is_respelt() {
    let db = Database::default();
    let text = section("own_source: [\"github.com/acme/\"]");
    let project = support::project(&db, Layout::Loose, &[("app.jadpo", &text)]);
    let found = jadpo_check::check(&db, project);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, RULE);
    assert_eq!(
        found[0].message,
        "`own_source` isn't something `packages:` holds"
    );
    assert_eq!(
        found[0].next.as_ref().map(|next| next.text.as_str()),
        Some("Use `own_sources` instead of `own_source`")
    );
}

/// The list goes in `access.jadpo_record`, sorted, one line per entry,
/// which gives the check its before side.
#[test]
fn the_list_is_kept_in_the_access_record() {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Project,
        &[(
            "app.jadpo",
            "application {\n    packages: {\n        own_sources: [\"github.com/acme/\", \"git.acme.example/platform/\"]\n    }\n}\n",
        )],
    );
    let record = jadpo_check::acceptances::access::record_text(&db, project)
        .clone()
        .flatten()
        .expect("a record");
    assert!(
        record.contains("own_source git.acme.example/platform/\nown_source github.com/acme/\n"),
        "{record}"
    );
}
