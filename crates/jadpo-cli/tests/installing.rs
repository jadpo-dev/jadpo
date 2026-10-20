//! Installing packages (area 11, "Installing safely" and "Developing and
//! publishing a package"; area 18, "The lock"; chunk 7.1b2): `jadpo
//! package add` from a path and from a git repository, verifying the
//! package's surface against its code; `update` to its newest release;
//! `remove`; and `link` and `unlink`, the app compiled against a live
//! checkout that CI refuses. Every repository is a local one in a temp
//! folder: nothing is fetched from the network.

use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_db::{Source, parse_lock};
use jadpo_term::Environment;

use super::http::{jadpo, tempdir, write};
use super::packages::{git, plain, repository};

/// A package's own repository with its surface written, and an app beside
/// it: `<dir>/postcodes` and `<dir>/shop`.
fn workspace(name: &str) -> (tempdir::Dir, PathBuf, PathBuf) {
    let (dir, package) = repository(name);
    let checked = plain(&package, &["package", "check"]);
    assert!(
        package.join("surface.jadpo_record").exists(),
        "{}",
        checked.stdout
    );
    let app = dir.path().join("shop");
    write(&app, "app.jadpo", "application {}\n");
    (dir, package, app)
}

/// Commits everything and tags it.
fn release(package: &Path, tag: &str) {
    git(package, &["add", "-A"]);
    git(package, &["commit", "-q", "-m", tag]);
    git(package, &["tag", tag]);
}

fn lock_lines(app: &Path) -> Vec<jadpo_db::Locked> {
    let text = std::fs::read_to_string(app.join("jadpo.lock")).expect("the lock");
    parse_lock(&text).expect("a lock that reads")
}

fn with_ci(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(
        &args,
        &Environment::empty().with_var("CI", "true").with_cwd(cwd),
    )
}

const USES: &str = "use packages/postcodes

/// Reads the postcode a customer typed.
action check_postcode(written: Postcodes.WrittenPostcode) fails Postcodes.InvalidPostcode -> Postcodes.UkPostcode {
    return attempt Postcodes.uk.parse(written)
}
";

#[test]
fn add_installs_a_package_from_a_path() {
    let (_dir, _package, app) = workspace("install-path");
    let added = jadpo(&app, &["package", "add", "../postcodes"]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    assert_eq!(
        added.stdout,
        "Added Postcodes.v2 2.0.0
    folder   packages/postcodes/v2
    source   path ../postcodes
    surface  matches its code

package added: Postcodes.v2 2.0.0, 1 function, 2 types, 1 failure

next: jadpo package show postcodes  shows what it offers
"
    );
    for file in [
        "package.jadpo",
        "libraries/uk.jadpo",
        "libraries/uk.test.jadpo",
        "surface.jadpo_record",
        "CHANGELOG.md",
    ] {
        assert!(
            app.join("packages/postcodes/v2").join(file).is_file(),
            "{file}"
        );
    }
    let lines = lock_lines(&app);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].package, "Postcodes.v2");
    assert!(
        matches!(&lines[0].source, Source::Path { version, path, .. } if version == "2.0.0" && path == "../postcodes"),
        "{:?}",
        lines[0]
    );
    // The app checks with the package installed, and uses it.
    write(&app, "domains/intake/actions/check_postcode.jadpo", USES);
    let checked = jadpo(&app, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);

    // Added twice is refused: an installed package moves with `update`.
    let again = jadpo(&app, &["package", "add", "../postcodes"]);
    assert_eq!(again.code, 2, "{}", again.stdout);
    assert!(
        again
            .stderr
            .starts_with("error: `Postcodes.v2` is already installed\n"),
        "{}",
        again.stderr
    );
}

#[test]
fn add_refuses_a_package_whose_surface_differs_from_its_code() {
    let (_dir, package, app) = workspace("install-surface");
    write(
        &package,
        "types/outward_code.jadpo",
        "/// The first half of a postcode, such as \"SW1A\".
public type OutwardCode = Text {
    max_length: 4
}
",
    );
    let refused = jadpo(&app, &["package", "add", "../postcodes"]);
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(
        refused.stderr.starts_with(
            "error: `Postcodes.v2`'s surface.jadpo_record doesn't match its code
  reason: Its code has `type OutwardCode`, which its committed surface doesn't. A package is installed only when its surface, what reviewers read, is what its code offers.
"
        ),
        "{}",
        refused.stderr
    );
    assert!(
        refused.stderr.contains("rule: package.surface_mismatch\n"),
        "{}",
        refused.stderr
    );
    assert!(!app.join("packages").exists());
    assert!(!app.join("jadpo.lock").exists());
}

#[test]
fn add_installs_from_a_git_repository_at_a_tag() {
    let (dir, package, app) = workspace("install-git");
    write(
        &package,
        "CHANGELOG.md",
        "# Changelog\n\n## 2.0.0\n\n- first release\n\nReads UK postcodes.\n",
    );
    release(&package, "v2.0.0");
    // A change after the tag isn't what the tag installs.
    write(
        &package,
        "GUIDE.md",
        "# Postcodes\n\nChanged after the tag.\n",
    );
    git(&package, &["commit", "-q", "-am", "after the tag"]);
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "v2.0.0^{commit}"])
        .current_dir(&package)
        .output()
        .expect("git");
    let commit = String::from_utf8_lossy(&commit.stdout).trim().to_string();
    let repository = package.to_string_lossy().into_owned();
    let added = jadpo(&app, &["package", "add", &format!("{repository}@v2.0.0")]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    assert!(
        added
            .stdout
            .contains(&format!("    source   git {repository} tag v2.0.0\n")),
        "{}",
        added.stdout
    );
    let lines = lock_lines(&app);
    match &lines[0].source {
        Source::Git {
            version,
            repository: repo,
            tag,
            commit: locked,
            ..
        } => {
            assert_eq!(version, "2.0.0");
            assert_eq!(repo, &repository);
            assert_eq!(tag, "v2.0.0");
            assert_eq!(locked, &commit);
        }
        other => panic!("{other:?}"),
    }
    let guide = std::fs::read_to_string(app.join("packages/postcodes/v2/GUIDE.md")).expect("guide");
    assert!(!guide.contains("after the tag"), "{guide}");
    assert!(!app.join("packages/postcodes/v2/.git").exists());
    assert_eq!(jadpo(&app, &["check"]).code, 0);

    // A tag the repository doesn't have is refused, naming its releases.
    let other = dir.path().join("other");
    write(&other, "app.jadpo", "application {}\n");
    let missing = jadpo(&other, &["package", "add", &format!("{repository}@v9.0.0")]);
    assert_eq!(missing.code, 1, "{}", missing.stderr);
    assert!(
        missing.stderr.contains("rule: package.fetch_failed\n"),
        "{}",
        missing.stderr
    );
    assert!(!other.join("jadpo.lock").exists());
}

#[test]
fn update_moves_a_git_package_to_its_newest_release() {
    let (_dir, package, app) = workspace("install-update");
    write(
        &package,
        "CHANGELOG.md",
        "# Changelog\n\n## 2.0.0\n\n- first release\n\nReads UK postcodes.\n",
    );
    release(&package, "v2.0.0");
    let repository = package.to_string_lossy().into_owned();
    let added = jadpo(&app, &["package", "add", &format!("{repository}@v2.0.0")]);
    assert_eq!(added.code, 0, "{}", added.stderr);
    let current = jadpo(&app, &["package", "update", "postcodes"]);
    assert_eq!(current.code, 0, "{}", current.stderr);
    assert!(
        current
            .stdout
            .starts_with("Postcodes.v2 is up to date: 2.0.0"),
        "{}",
        current.stdout
    );
    write(
        &package,
        "types/outward_code.jadpo",
        "/// The first half of a postcode, such as \"SW1A\".
public type OutwardCode = Text {
    max_length: 4
}
",
    );
    let checked = plain(&package, &["package", "check"]);
    assert!(
        checked.stdout.contains("2.1.0, 1 added since 2.0.0"),
        "{}",
        checked.stdout
    );
    let log = std::fs::read_to_string(package.join("CHANGELOG.md")).expect("changelog");
    write(
        &package,
        "CHANGELOG.md",
        &log.replace(
            "- added type `OutwardCode`\n",
            "- added type `OutwardCode`\n\nReads the outward code.\n",
        ),
    );
    release(&package, "v2.1.0");
    let updated = jadpo(&app, &["package", "update", "postcodes"]);
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    assert!(
        updated
            .stdout
            .starts_with("Updated Postcodes.v2 from 2.0.0 to 2.1.0\n"),
        "{}",
        updated.stdout
    );
    assert!(
        updated.stdout.contains("  + type OutwardCode"),
        "{}",
        updated.stdout
    );
    assert!(
        matches!(&lock_lines(&app)[0].source, Source::Git { version, tag, .. } if version == "2.1.0" && tag == "v2.1.0")
    );
    assert!(
        app.join("packages/postcodes/v2/types/outward_code.jadpo")
            .is_file()
    );
    assert_eq!(jadpo(&app, &["check"]).code, 0);
}

#[test]
fn remove_refuses_a_package_the_app_uses() {
    let (_dir, _package, app) = workspace("install-remove");
    assert_eq!(jadpo(&app, &["package", "add", "../postcodes"]).code, 0);
    write(&app, "domains/intake/actions/check_postcode.jadpo", USES);
    let refused = jadpo(&app, &["package", "remove", "postcodes"]);
    assert_eq!(refused.code, 1, "{}", refused.stdout);
    assert!(
        refused
            .stderr
            .starts_with("error: The app still uses `Postcodes.v2`\n"),
        "{}",
        refused.stderr
    );
    assert!(
        refused.stderr.contains("rule: package.in_use\n"),
        "{}",
        refused.stderr
    );
    assert!(app.join("packages/postcodes/v2").is_dir());

    std::fs::remove_dir_all(app.join("domains")).expect("remove the domain");
    let removed = jadpo(&app, &["package", "remove", "postcodes"]);
    assert_eq!(removed.code, 0, "{}{}", removed.stdout, removed.stderr);
    assert_eq!(
        removed.stdout,
        "Removed packages/postcodes/v2 and its line in jadpo.lock

package removed: Postcodes.v2 2.0.0
"
    );
    assert!(!app.join("packages").exists());
    assert!(lock_lines(&app).is_empty());
    assert_eq!(jadpo(&app, &["check"]).code, 0);
}

#[test]
fn link_compiles_the_app_against_a_checkout_and_unlink_restores_it() {
    let (dir, package, app) = workspace("install-link");
    let added = jadpo(&app, &["package", "add", "../postcodes"]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    let installed = std::fs::read_to_string(app.join("jadpo.lock")).expect("lock");
    // A live checkout beside it, with a type the installed version lacks.
    let checkout = dir.path().join("postcodes-dev");
    for entry in ["package.jadpo", "types", "failures", "libraries"] {
        copy(&package.join(entry), &checkout.join(entry));
    }
    write(
        &checkout,
        "types/outward_code.jadpo",
        "/// The first half of a postcode, such as \"SW1A\".
public type OutwardCode = Text {
    max_length: 4
}
",
    );
    write(
        &app,
        "domains/intake/actions/outward.jadpo",
        "use packages/postcodes

/// The outward code a customer typed.
action outward(code: Postcodes.OutwardCode) -> Postcodes.OutwardCode {
    return code
}
",
    );
    assert_eq!(jadpo(&app, &["check"]).code, 1);
    let linked = jadpo(&app, &["package", "link", "../postcodes-dev"]);
    assert_eq!(linked.code, 0, "{}{}", linked.stdout, linked.stderr);
    assert!(
        linked.stdout.starts_with(
            "Linked Postcodes.v2 to ../postcodes-dev
    checkout   ../postcodes-dev
    installed  2.0.0, restored by jadpo package unlink postcodes
"
        ),
        "{}",
        linked.stdout
    );
    assert!(
        matches!(&lock_lines(&app)[0].source, Source::Link { path } if path == "../postcodes-dev")
    );
    // Every check compiles the app against the checkout.
    let checked = jadpo(&app, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    // CI refuses a link, so one never ships.
    let ci = with_ci(&app, &["check"]);
    assert_eq!(ci.code, 1, "{}", ci.stdout);
    assert!(
        ci.stdout
            .starts_with("error: `Postcodes.v2` is linked to `../postcodes-dev`\n"),
        "{}",
        ci.stdout
    );
    assert!(
        ci.stdout.contains("rule: package.link_refused\n"),
        "{}",
        ci.stdout
    );
    let ci_test = with_ci(&app, &["test"]);
    assert_eq!(ci_test.code, 1, "{}{}", ci_test.stdout, ci_test.stderr);
    assert!(
        format!("{}{}", ci_test.stdout, ci_test.stderr).contains("rule: package.link_refused"),
        "{}{}",
        ci_test.stdout,
        ci_test.stderr
    );

    let unlinked = jadpo(&app, &["package", "unlink", "postcodes"]);
    assert_eq!(unlinked.code, 0, "{}{}", unlinked.stdout, unlinked.stderr);
    assert!(
        unlinked
            .stdout
            .starts_with("Unlinked Postcodes.v2 from ../postcodes-dev\n"),
        "{}",
        unlinked.stdout
    );
    assert_eq!(
        std::fs::read_to_string(app.join("jadpo.lock")).expect("lock"),
        installed
    );
}

#[test]
fn package_commands_name_what_they_cant_find() {
    let (_dir, _package, app) = workspace("install-arguments");
    let none = jadpo(&app, &["package", "remove", "postcodes"]);
    assert_eq!(none.code, 2);
    assert!(
        none.stderr
            .starts_with("error: `postcodes` isn't an installed package\n"),
        "{}",
        none.stderr
    );
    let folder = jadpo(&app, &["package", "add", "../nowhere"]);
    assert_eq!(folder.code, 2);
    assert!(
        folder
            .stderr
            .starts_with("error: `../nowhere` isn't a package's folder or a git repository\n"),
        "{}",
        folder.stderr
    );
    let registry = jadpo(&app, &["package", "add", "postcodes"]);
    assert_eq!(registry.code, 1);
    assert!(
        registry
            .stderr
            .starts_with("error: Installing from the registry isn't built yet\n"),
        "{}",
        registry.stderr
    );
}

fn copy(from: &Path, to: &Path) {
    if from.is_dir() {
        for entry in std::fs::read_dir(from).expect("read") {
            let entry = entry.expect("entry");
            copy(&entry.path(), &to.join(entry.file_name()));
        }
    } else {
        std::fs::create_dir_all(to.parent().expect("parent")).expect("folders");
        std::fs::copy(from, to).expect("copy");
    }
}
