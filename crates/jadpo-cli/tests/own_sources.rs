//! Your own company's git packages, end to end through git and `jadpo
//! package add` (area 11, "Installing safely"; area 18, "Acceptances"): a
//! package from a repository under one of `packages:`'s `own_sources:`
//! needs no `PackageChange.git_source`; adding or widening an entry needs
//! `PolicyChange.own_source_added`, so an agent can't let a stranger in by
//! adding a line; and no lookalike address counts as under one.

use std::fs;
use std::path::{Path, PathBuf};

use super::acceptances::{acceptance_files, edit, git, jadpo, write};
use super::http::tempdir;
use super::installing::{release, workspace};

const OWN: &str = "acceptance.missing: a new own source lets its repositories in without a person";
const GIT: &str =
    "acceptance.missing: a package installed from git has no verified publisher: Postcodes.v2";

fn problems(root: &Path) -> Vec<String> {
    let output = jadpo(root, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|d| {
            format!(
                "{}: {}",
                d["rule"].as_str().unwrap_or(""),
                d["message"].as_str().unwrap_or("")
            )
        })
        .collect()
}

/// An app in git on a branch, and a release of a package beside it not yet
/// added, whose repository is the folder `<dir>/postcodes`.
fn app(name: &str) -> (tempdir::Dir, PathBuf, String) {
    let (dir, package, app) = workspace(name);
    release(&package, "v2.0.0");
    git(&app, &["init", "-q"]);
    git(&app, &["add", "-A"]);
    git(&app, &["commit", "-q", "-m", "The app"]);
    git(&app, &["checkout", "-q", "-b", "change"]);
    let repository = package.to_string_lossy().into_owned();
    (dir, app, repository)
}

/// `jadpo package add`, with the lock then saying the package came from
/// `address`, as it would have fetched from there.
fn add(app: &Path, repository: &str, address: &str) {
    let added = jadpo(app, &["package", "add", &format!("{repository}@v2.0.0")]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    relock(app, repository, address);
}

fn relock(app: &Path, from: &str, to: &str) {
    edit(
        app,
        "jadpo.lock",
        &format!(" git {from} "),
        &format!(" git {to} "),
    );
}

fn own_sources(app: &Path, entries: &[&str]) {
    let list: Vec<String> = entries.iter().map(|e| format!("\"{e}\"")).collect();
    write(
        app,
        "app.jadpo",
        &format!(
            "application {{\n    packages: {{\n        own_sources: [{}]\n    }}\n}}\n",
            list.join(", ")
        ),
    );
    let recorded = jadpo(app, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
}

/// Everything on the branch, merged to `main`, and a new branch off it.
fn merged(app: &Path, branch: &str) {
    git(app, &["add", "-A"]);
    git(app, &["commit", "-q", "-m", "Merged"]);
    git(app, &["checkout", "-q", "main"]);
    git(app, &["merge", "-q", "--ff-only", "change"]);
    git(app, &["checkout", "-q", "-b", branch]);
}

/// What a person agreeing does: `jadpo acceptance new`, with a reason.
fn accept(app: &Path, purpose: &str) {
    let drafted = jadpo(app, &["acceptance", "new", purpose]);
    assert_eq!(drafted.code, 0, "{}{}", drafted.stdout, drafted.stderr);
    for file in acceptance_files(app) {
        let text = fs::read_to_string(&file).expect("read");
        if !text.starts_with("///") {
            fs::write(&file, format!("/// A person agreed.\n{text}")).expect("written");
        }
    }
}

#[test]
fn a_package_from_a_stranger_needs_a_person_even_as_the_apps_first() {
    // No `jadpo.lock` on the before side reads as an empty one.
    let (_dir, app, repository) = app("own-source-first");
    add(&app, &repository, "github.com/stranger/postcodes");
    assert_eq!(problems(&app), [GIT]);
    accept(&app, "use_their_postcodes");
    assert_eq!(problems(&app), Vec::<String>::new());
}

#[test]
fn an_own_source_lets_its_packages_in_and_needs_a_person_itself() {
    let (_dir, app, repository) = app("own-source-added");
    add(&app, &repository, "github.com/acme/postcodes");
    own_sources(&app, &["github.com/acme/"]);
    let record = fs::read_to_string(app.join("access.jadpo_record")).expect("the record");
    assert!(record.contains("own_source github.com/acme/\n"), "{record}");
    // The package is no stranger's now; the list is what a person accepts.
    assert_eq!(problems(&app), [format!("{OWN}: \"github.com/acme/\"")]);
    accept(&app, "trust_our_org");
    let files = acceptance_files(&app);
    let text = fs::read_to_string(&files[0]).expect("read");
    assert!(
        text.contains("\"github.com/acme/\": PolicyChange.own_source_added"),
        "{text}"
    );
    assert!(!text.contains("git_source"), "{text}");
    assert_eq!(problems(&app), Vec::<String>::new());
    // An entry for a package the list covers has nothing to match.
    edit(
        &app,
        &format!(
            "acceptances/{}",
            files[0].file_name().and_then(|n| n.to_str()).unwrap_or("")
        ),
        "    }\n",
        "        Postcodes.v2: PackageChange.git_source\n    }\n",
    );
    assert_eq!(
        problems(&app),
        ["acceptance.unmatched: `Postcodes.v2: PackageChange.git_source` matches no change"]
    );
}

#[test]
fn a_lookalike_of_an_own_source_needs_a_person() {
    let (_dir, app, repository) = app("own-source-lookalike");
    add(&app, &repository, "github.com/acme/postcodes");
    own_sources(&app, &["github.com/acme/"]);
    accept(&app, "trust_our_org");
    assert_eq!(problems(&app), Vec::<String>::new());
    let mut was = "github.com/acme/postcodes".to_string();
    for lookalike in [
        "github.com/acme-evil/postcodes",
        "github.com/ACME/postcodes",
        "github.com/acme.git/postcodes",
        "github.com@evil.example/acme/postcodes",
        "https://token@github.com/acme/postcodes",
        "github.com:8443/acme/postcodes",
        "github.com/acme/../evil/postcodes",
        "http://github.com/acme/postcodes",
    ] {
        relock(&app, &was, lookalike);
        was = lookalike.to_string();
        assert_eq!(problems(&app), [GIT], "{lookalike}");
    }
    for same in [
        "github.com/acme/postcodes.git",
        "https://github.com/acme/postcodes",
        "git@github.com:acme/postcodes.git",
        "GitHub.com/acme/postcodes",
    ] {
        relock(&app, &was, same);
        was = same.to_string();
        assert_eq!(problems(&app), Vec::<String>::new(), "{same}");
    }
}

#[test]
fn removing_or_narrowing_an_own_source_needs_nothing_and_widening_needs_a_person() {
    let (_dir, app, repository) = app("own-source-widen");
    own_sources(
        &app,
        &["github.com/acme/tools/", "git.acme.example/platform/"],
    );
    add(&app, &repository, "github.com/acme/tools/postcodes");
    accept(&app, "trust_our_org");
    assert_eq!(problems(&app), Vec::<String>::new());
    merged(&app, "next");
    assert_eq!(problems(&app), Vec::<String>::new());

    // Widened: the entry that covers more is what a person accepts.
    own_sources(&app, &["github.com/acme/", "git.acme.example/platform/"]);
    assert_eq!(problems(&app), [format!("{OWN}: \"github.com/acme/\"")]);
    // A sibling is as new as a stranger.
    own_sources(
        &app,
        &["github.com/acme/tools/", "github.com/acme/tools-evil/"],
    );
    assert_eq!(
        problems(&app),
        [format!("{OWN}: \"github.com/acme/tools-evil/\"")]
    );
    // Removed, and narrowed: nothing new gets in.
    own_sources(&app, &["github.com/acme/tools/deep/"]);
    assert_eq!(problems(&app), Vec::<String>::new());
    // The package already locked keeps its place when its source goes.
    own_sources(&app, &[]);
    assert_eq!(problems(&app), Vec::<String>::new());
}

#[test]
fn a_package_moved_out_of_an_own_source_needs_a_person() {
    let (_dir, app, repository) = app("own-source-moved");
    own_sources(&app, &["github.com/acme/"]);
    add(&app, &repository, "github.com/acme/postcodes");
    accept(&app, "trust_our_org");
    merged(&app, "next");
    relock(
        &app,
        "github.com/acme/postcodes",
        "github.com/stranger/postcodes",
    );
    assert_eq!(problems(&app), [GIT]);
    // To another of the company's repositories: still its own.
    relock(
        &app,
        "github.com/stranger/postcodes",
        "github.com/acme/postcodes-fork",
    );
    assert_eq!(problems(&app), Vec::<String>::new());
}

/// The manifest says which entry let a package in, for a reviewer.
#[test]
fn the_manifest_names_the_own_source() {
    let (_dir, app, repository) = app("own-source-manifest");
    own_sources(&app, &["github.com/acme/"]);
    add(&app, &repository, "github.com/acme/postcodes");
    let output = jadpo(&app, &["check"]);
    let manifest = fs::read_to_string(app.join("build/manifest.json")).unwrap_or_else(|error| {
        panic!("{error}: {}{}", output.stdout, output.stderr);
    });
    assert!(
        manifest.contains("\"ownSource\": \"github.com/acme/\""),
        "{manifest}"
    );
}
