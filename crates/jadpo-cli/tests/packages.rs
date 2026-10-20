//! `jadpo package show` and `jadpo package check` (area 11, "The surface"
//! and "Developing and publishing a package"; chunk 7.1b): a package's
//! description, licence, version, kind and public surface, one declaration
//! in full, its guide; and the publishing bar run in a package's own
//! repository, writing `surface.jadpo_record` with the version its changes
//! since the last tag allow.

use std::path::Path;
use std::process::Command;

use jadpo_cli::{Output, run};
use jadpo_db::{Locked, Source, package_digest, write_lock};
use jadpo_term::Environment;

use super::http::{jadpo, tempdir, write};

pub(super) const INDEX: &str = "/// UK postcodes: reading and normalising them.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: \"1.0\"
}
";

const POSTCODE: &str = "/// A UK postcode in standard form, such as \"SW1A 1AA\".
public type UkPostcode = Text {
    min_length: 5
    max_length: 8
}
";

const WRITTEN: &str = "/// A postcode as someone typed it.
public type WrittenPostcode = Text {
    max_length: 20
}
";

const INVALID: &str = "/// The text isn't a UK postcode.
public type InvalidPostcode = Failure {
    kind: FailureKind.invalid_value
}
";

pub(super) const UK: &str = "/// Reading UK postcodes.
public library uk {
    /// Reads a UK postcode as it is written.
    ///
    /// Spaces and case don't matter: \"sw1a1aa\" reads as \"SW1A 1AA\".
    function parse(written: WrittenPostcode) fails InvalidPostcode -> UkPostcode {
        return attempt UkPostcode(text.upper(written)) {
            invalid: InvalidPostcode
        }
    }
}
";

/// `uk` with an example in `parse`'s description.
pub(super) const UK_EXAMPLE: &str = "/// Reading UK postcodes.
public library uk {
    /// Reads a UK postcode as it is written.
    ///
    /// ```jadpo
    /// var postcode: UkPostcode = attempt uk.parse(\"sw1a 1aa\")
    /// assert postcode == \"SW1A 1AA\"
    /// assert uk.parse(\"x\") fails InvalidPostcode
    /// ```
    function parse(written: WrittenPostcode) fails InvalidPostcode -> UkPostcode {
        return attempt UkPostcode(text.upper(written)) {
            invalid: InvalidPostcode
        }
    }
}
";

const GUIDE: &str = "# Postcodes\n\nParse what people type, then store the `UkPostcode`.\n";

/// The package's files, relative to its folder.
pub(super) fn package_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("package.jadpo", INDEX),
        ("types/uk_postcode.jadpo", POSTCODE),
        ("types/written_postcode.jadpo", WRITTEN),
        ("failures/invalid_postcode.jadpo", INVALID),
        ("libraries/uk.jadpo", UK),
        ("GUIDE.md", GUIDE),
    ]
}

/// An app with the package installed from `../postcodes` at 2.1.0.
fn app(name: &str) -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new(name);
    let root = dir.path().join("shop");
    write(&root, "app.jadpo", "application {}\n");
    for (path, text) in package_files() {
        write(&root, &format!("packages/postcodes/v2/{path}"), text);
    }
    let held: Vec<(&str, &[u8])> = package_files()
        .iter()
        .map(|(path, text)| (*path, text.as_bytes()))
        .collect();
    let lock = write_lock(&[Locked {
        package: "Postcodes.v2".to_string(),
        source: Source::Path {
            version: "2.1.0".to_string(),
            path: "../postcodes".to_string(),
            digest: package_digest(&held),
        },
    }]);
    write(&root, "jadpo.lock", &lock);
    (dir, root)
}

#[test]
fn show_prints_a_packages_description_and_public_surface() {
    let (_dir, root) = app("package-show");
    let done = jadpo(&root, &["package", "show", "postcodes"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert_eq!(
        done.stdout,
        "UK postcodes: reading and normalising them.
  package: Postcodes.v2
  kind: library
  version: 2.1.0
  source: path ../postcodes
  licence: mit
  jadpo: 1.0
  folder: packages/postcodes/v2
  guide: GUIDE.md

Functions
  uk.parse         (written: WrittenPostcode) fails InvalidPostcode -> UkPost...

Types
  UkPostcode       Text { min_length: 5, max_length: 8 }
  WrittenPostcode  Text { max_length: 20 }

Failures
  InvalidPostcode  invalid_value

Postcodes.v2: 1 function, 2 types, 1 failure

next: jadpo package show postcodes uk.parse  shows one in full, with its file
"
    );
    // Its name in code finds it too, with or without its version.
    for name in ["Postcodes", "Postcodes.v2"] {
        assert_eq!(
            jadpo(&root, &["package", "show", name]).stdout,
            done.stdout,
            "{name}"
        );
    }
    // `packages` is a synonym.
    assert_eq!(
        jadpo(&root, &["packages", "show", "postcodes"]).stdout,
        done.stdout
    );
}

#[test]
fn show_prints_one_declaration_in_full() {
    let (_dir, root) = app("package-show-one");
    let done = jadpo(&root, &["package", "show", "postcodes", "uk.parse"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert_eq!(
        done.stdout,
        "Reads a UK postcode as it is written.
  name: uk.parse
  kind: function
  package: Postcodes.v2
  file: packages/postcodes/v2/libraries/uk.jadpo:6
  takes: written: WrittenPostcode
  returns: UkPostcode
  fails: InvalidPostcode

Description
  Reads a UK postcode as it is written.

  Spaces and case don't matter: \"sw1a1aa\" reads as \"SW1A 1AA\".
"
    );
    let ty = jadpo(&root, &["package", "show", "postcodes", "UkPostcode"]);
    assert_eq!(
        ty.stdout,
        "A UK postcode in standard form, such as \"SW1A 1AA\".
  name: UkPostcode
  kind: type
  package: Postcodes.v2
  file: packages/postcodes/v2/types/uk_postcode.jadpo:2
  built on: Text
  rules: min_length: 5, max_length: 8
"
    );
    let failure = jadpo(&root, &["package", "show", "postcodes", "InvalidPostcode"]);
    assert_eq!(
        failure.stdout,
        "The text isn't a UK postcode.
  name: InvalidPostcode
  kind: failure
  package: Postcodes.v2
  file: packages/postcodes/v2/failures/invalid_postcode.jadpo:2
  failure: invalid_value
  code: invalid_postcode
"
    );
}

#[test]
fn show_prints_the_guide_and_gives_json() {
    let (_dir, root) = app("package-show-guide");
    let guide = jadpo(&root, &["package", "show", "postcodes", "--guide"]);
    assert_eq!(guide.code, 0, "{}", guide.stderr);
    assert_eq!(guide.stdout, GUIDE);
    let json = jadpo(&root, &["package", "show", "postcodes", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    let data = &document["data"];
    assert_eq!(data["package"]["name"], "Postcodes.v2");
    assert_eq!(data["package"]["kind"], "library");
    assert_eq!(data["package"]["version"], "2.1.0");
    assert_eq!(data["package"]["source"], "path ../postcodes");
    assert_eq!(data["package"]["folder"], "packages/postcodes/v2");
    let names: Vec<&str> = data["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .map(|entry| entry["name"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        names,
        [
            "uk.parse",
            "UkPostcode",
            "WrittenPostcode",
            "InvalidPostcode"
        ]
    );
    let parse = &data["declarations"][0];
    assert_eq!(parse["kind"], "function");
    assert_eq!(parse["file"], "packages/postcodes/v2/libraries/uk.jadpo");
    assert_eq!(
        parse["facts"][0],
        "signature (written: WrittenPostcode) -> UkPostcode"
    );
}

#[test]
fn show_works_for_the_apps_own_integration() {
    let dir = tempdir::Dir::new("package-show-own");
    let root = dir.path().join("shop");
    write(&root, "app.jadpo", "application {}\n");
    for (path, text) in package_files() {
        write(
            &root,
            &format!("shared/providers/postcodes/v2/{path}"),
            text,
        );
    }
    let done = jadpo(&root, &["package", "show", "postcodes"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert!(
        done.stdout.starts_with(
            "UK postcodes: reading and normalising them.
  package: Postcodes.v2
  kind: library
  source: the app's own
  licence: mit
  jadpo: 1.0
  folder: shared/providers/postcodes/v2
  guide: GUIDE.md
"
        ),
        "{}",
        done.stdout
    );
    // `jadpo check` writes the app's own integration's surface to
    // `build/`, where it is generated rather than committed.
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    let surface =
        std::fs::read_to_string(root.join("build/providers/postcodes/v2/surface.jadpo_record"))
            .expect("the surface in build/");
    assert!(
        surface.contains("\npackage Postcodes.v2\n") && surface.contains("\nfunction uk.parse\n"),
        "{surface}"
    );
}

#[test]
fn show_names_what_it_cant_find() {
    let (_dir, root) = app("package-show-missing");
    let unknown = jadpo(&root, &["package", "show", "postcode"]);
    assert_eq!(unknown.code, 2);
    assert_eq!(
        unknown.stderr,
        "error: `postcode` isn't a package in this project
  reason: The project holds `postcodes`. Did you mean `postcodes`?
  next: Use `jadpo package show postcodes` (agent)
  rule: cli.package_arguments
  help: docs/language/14-tooling.html#command-errors
"
    );
    let declaration = jadpo(&root, &["package", "show", "postcodes", "uk.pars"]);
    assert_eq!(declaration.code, 2);
    assert_eq!(
        declaration.stderr,
        "error: `Postcodes.v2` has no public `uk.pars`
  reason: Its public surface holds `uk.parse`, `UkPostcode`, `WrittenPostcode` and `InvalidPostcode`. Did you mean `uk.parse`?
  next: Use `jadpo package show postcodes uk.parse` (agent)
  rule: cli.package_arguments
  help: docs/language/14-tooling.html#command-errors
"
    );
    let none = jadpo(&root, &["package", "show"]);
    assert_eq!(none.code, 2);
    assert!(
        none.stderr
            .starts_with("error: `jadpo package show` needs a package\n"),
        "{}",
        none.stderr
    );
    let later = jadpo(&root, &["package", "new", "Postcodes"]);
    assert_eq!(later.code, 1);
    assert!(
        later
            .stderr
            .starts_with("error: `jadpo package new` isn't built yet\n"),
        "{}",
        later.stderr
    );
}

// --- jadpo package check ---------------------------------------------------

pub(super) fn git(folder: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=Jadpo Tests",
            "-c",
            "user.email=tests@jadpo.example",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
        ])
        .args(args)
        .current_dir(folder)
        .output()
        .expect("git runs");
    assert!(
        status.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

/// A package's own repository: the package at its root.
pub(super) fn repository(name: &str) -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new(name);
    let root = dir.path().join("postcodes");
    for (path, text) in package_files() {
        write(&root, path, text);
    }
    write(&root, "libraries/uk.jadpo", UK_EXAMPLE);
    write(
        &root,
        "libraries/uk.test.jadpo",
        "test \"a postcode is read in upper case\" {
    var postcode: UkPostcode = attempt uk.parse(\"sw1a 1aa\")
    assert postcode == \"SW1A 1AA\"
    assert uk.parse(\"x\") fails InvalidPostcode
}
",
    );
    git(&root, &["init", "-q", "-b", "main"]);
    (dir, root)
}

pub(super) fn plain(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

#[test]
fn check_runs_the_publishing_bar_and_writes_the_surface() {
    let (_dir, root) = repository("package-check");
    let first = plain(&root, &["package", "check"]);
    assert_eq!(first.code, 1, "{}{}", first.stdout, first.stderr);
    assert_eq!(
        first.stdout,
        "error: CHANGELOG.md doesn't say why 2.0.0 is released
 --> CHANGELOG.md:3:1
  |
3 | ## 2.0.0
  | ^^^^^^^^ no why beneath the list
  reason: `jadpo package check` writes what changed from the surface; whoever releases it writes why, beneath it. A release reviewers can't tell the purpose of is one they can't judge.
  next: Write why 2.0.0 is released under `## 2.0.0` in CHANGELOG.md, after the list (agent)
  rule: publish.changelog_missing
  help: docs/language/14-tooling.html#command-errors

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  examples      passed                        1 example, every failure shown
  tests         passed                        1 test
  version       passed                        2.0.0, the first release
  changelog     failed                        2.0.0 doesn't say why
  licence       passed                        mit, from Jadpo 1.0

Wrote surface.jadpo_record and CHANGELOG.md

package check failed: 1 failed, 6 passed
"
    );
    let surface = std::fs::read_to_string(root.join("surface.jadpo_record")).expect("surface");
    assert!(
        surface.starts_with(
            "// Generated by Jadpo from this package's code; update it with `jadpo package check`, never by hand.
record surface 1

package Postcodes.v2
    /// UK postcodes: reading and normalising them.
    file package.jadpo
    kind library
    version 2.0.0
"
        ),
        "{surface}"
    );
    let log = std::fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog");
    assert_eq!(log, "# Changelog\n\n## 2.0.0\n\n- first release\n");
    // The author says why; the list Jadpo wrote stays above it.
    write(
        &root,
        "CHANGELOG.md",
        &format!("{log}\nReads UK postcodes as people type them.\n"),
    );
    let second = plain(&root, &["package", "check"]);
    assert_eq!(second.code, 0, "{}", second.stdout);
    assert_eq!(
        second.stdout,
        "Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  examples      passed                        1 example, every failure shown
  tests         passed                        1 test
  version       passed                        2.0.0, the first release
  changelog     passed                        2.0.0 says why
  licence       passed                        mit, from Jadpo 1.0

package check passed: Postcodes.v2 2.0.0, 7 passed

next: git tag v2.0.0  tags the release once surface.jadpo_record is committed
"
    );

    // Released as v2.0.0; a new public type is a minor release.
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "first release"]);
    git(&root, &["tag", "v2.0.0"]);
    let tagged = plain(&root, &["package", "check"]);
    assert_eq!(tagged.code, 0, "{}", tagged.stdout);
    assert!(
        tagged
            .stdout
            .contains("  version       passed                        2.0.0, released as v2.0.0\n"),
        "{}",
        tagged.stdout
    );
    assert!(
        tagged
            .stdout
            .contains("  changelog     passed                        2.0.0 says why\n"),
        "{}",
        tagged.stdout
    );
    assert!(
        !tagged.stdout.contains("next: git tag"),
        "{}",
        tagged.stdout
    );
    write(
        &root,
        "types/outward_code.jadpo",
        "/// The first half of a postcode, such as \"SW1A\".
public type OutwardCode = Text {
    max_length: 4
}
",
    );
    let added = plain(&root, &["package", "check"]);
    assert_eq!(added.code, 1, "{}", added.stdout);
    assert!(
        added
            .stdout
            .contains("  version       passed                        2.1.0, 1 added since 2.0.0\n"),
        "{}",
        added.stdout
    );
    assert!(
        std::fs::read_to_string(root.join("surface.jadpo_record"))
            .expect("surface")
            .contains("    version 2.1.0\n")
    );
    // A section for 2.1.0 above 2.0.0's, which is kept as it was.
    assert_eq!(
        std::fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog"),
        "# Changelog

## 2.1.0

- added type `OutwardCode`

## 2.0.0

- first release

Reads UK postcodes as people type them.
"
    );
    // Another change before the tag rewrites the list and keeps the why.
    let log = std::fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog");
    write(
        &root,
        "CHANGELOG.md",
        &log.replace(
            "- added type `OutwardCode`\n",
            "- added type `OutwardCode`\n\nReads the outward code.\n",
        ),
    );
    write(
        &root,
        "types/inward_code.jadpo",
        "/// The second half of a postcode, such as \"1AA\".
public type InwardCode = Text {
    max_length: 3
}
",
    );
    let more = plain(&root, &["package", "check"]);
    assert_eq!(more.code, 0, "{}", more.stdout);
    assert!(
        std::fs::read_to_string(root.join("CHANGELOG.md"))
            .expect("changelog")
            .starts_with(
                "# Changelog

## 2.1.0

- added type `InwardCode`
- added type `OutwardCode`

Reads the outward code.

## 2.0.0
"
            )
    );
}

#[test]
fn check_runs_each_example_and_needs_one_for_every_function() {
    let (_dir, root) = repository("package-check-examples");
    write(&root, "libraries/uk.jadpo", UK);
    let missing = plain(&root, &["package", "check"]);
    assert_eq!(missing.code, 1, "{}", missing.stdout);
    assert!(
        missing.stdout.starts_with(
            "error: Function `uk.parse` has no example
 --> libraries/uk.jadpo:6:14
  |
6 |     function parse(written: WrittenPostcode) fails InvalidPostcode -> UkPostcode {
  |              ^^^^^ no example in its description
"
        ),
        "{}",
        missing.stdout
    );
    assert!(missing.stdout.contains("rule: publish.example_missing\n"));
    assert!(
        missing
            .stdout
            .contains("  examples      failed                        1 function has no example\n"),
        "{}",
        missing.stdout
    );

    // An example that doesn't hold fails the bar at its line.
    write(
        &root,
        "libraries/uk.jadpo",
        &UK_EXAMPLE.replace(
            "assert postcode == \"SW1A 1AA\"",
            "assert postcode == \"SW1A1AA\"",
        ),
    );
    let failed = plain(&root, &["package", "check"]);
    assert_eq!(failed.code, 1, "{}", failed.stdout);
    assert!(
        failed.stdout.starts_with(
            "error: Example of `uk.parse` failed
 --> libraries/uk.jadpo:7:9
  |
7 |     /// assert postcode == \"SW1A1AA\"
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ this doesn't hold
"
        ),
        "{}",
        failed.stdout
    );
    assert!(failed.stdout.contains("rule: publish.example_failed\n"));
    assert!(
        failed
            .stdout
            .contains("  examples      failed                        1 of 1 example failed\n"),
        "{}",
        failed.stdout
    );

    // One that doesn't compile is shown at its line too.
    write(
        &root,
        "libraries/uk.jadpo",
        &UK_EXAMPLE.replace(
            "assert postcode == \"SW1A 1AA\"",
            "assert postcode == missing_name",
        ),
    );
    let broken = plain(&root, &["package", "check"]);
    assert_eq!(broken.code, 1, "{}", broken.stdout);
    assert!(
        broken.stdout.starts_with(
            "error: Example of `uk.parse` doesn't compile
 --> libraries/uk.jadpo:7:9
"
        ),
        "{}",
        broken.stdout
    );

    // A failure no example or test shows is missing an example.
    write(
        &root,
        "libraries/uk.jadpo",
        &UK_EXAMPLE.replace("    /// assert uk.parse(\"x\") fails InvalidPostcode\n", ""),
    );
    write(
        &root,
        "libraries/uk.test.jadpo",
        "test \"a postcode is read in upper case\" {
    var postcode: UkPostcode = attempt uk.parse(\"sw1a 1aa\")
    assert postcode == \"SW1A 1AA\"
}
",
    );
    let unshown = plain(&root, &["package", "check"]);
    assert_eq!(unshown.code, 1, "{}", unshown.stdout);
    assert!(
        unshown.stdout.starts_with(
            "error: `uk.parse` can fail with `InvalidPostcode`, which no example or test shows
"
        ),
        "{}",
        unshown.stdout
    );
    assert!(
        unshown
            .stdout
            .contains("  examples      failed                        1 failure not shown\n"),
        "{}",
        unshown.stdout
    );
}

#[test]
fn check_refuses_a_breaking_change_within_a_major_version() {
    let (_dir, root) = repository("package-check-breaking");
    plain(&root, &["package", "check"]);
    let log = std::fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog");
    write(
        &root,
        "CHANGELOG.md",
        &format!("{log}\nReads UK postcodes.\n"),
    );
    assert_eq!(plain(&root, &["package", "check"]).code, 0);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "first release"]);
    git(&root, &["tag", "v2.0.0"]);
    std::fs::remove_file(root.join("types/written_postcode.jadpo")).expect("remove");
    write(
        &root,
        "types/entered_postcode.jadpo",
        "/// A postcode as someone typed it.
public type EnteredPostcode = Text {
    max_length: 20
}
",
    );
    write(
        &root,
        "libraries/uk.jadpo",
        &UK_EXAMPLE.replace("written: WrittenPostcode", "written: EnteredPostcode"),
    );
    let refused = plain(&root, &["package", "check"]);
    assert_eq!(refused.code, 1, "{}", refused.stdout);
    assert!(
        refused.stdout.starts_with(
            "error: `Postcodes.v2` changed in ways that break the apps that use it
  reason: Since 2.0.0, `function uk.parse` changed and `type WrittenPostcode` was removed, which an app using 2.0.0 may rely on. A breaking change is a new major version: a new package beside this one.
  next: Put back what changed, or publish the change as `Postcodes.v3` in a folder of its own (human)
  rule: publish.breaking_change
  help: docs/language/14-tooling.html#command-errors
"
        ),
        "{}",
        refused.stdout
    );
    assert!(
        refused.stdout.contains(
            "  version       failed                        2 breaking changes since 2.0.0\n"
        ),
        "{}",
        refused.stdout
    );
    // The surface isn't written for a version the change can't have.
    assert!(
        std::fs::read_to_string(root.join("surface.jadpo_record"))
            .expect("surface")
            .contains("    version 2.0.0\n")
    );
}

#[test]
fn check_shows_each_problem_at_its_place_in_the_package() {
    let (_dir, root) = repository("package-check-problems");
    write(
        &root,
        "types/outward_code.jadpo",
        "public type OutwardCode = Text {
    max_length: 4
}
",
    );
    write(
        &root,
        "types/geo_point.jadpo",
        "/// Where a postcode's centre is.
public type GeoPoint = Object {
    lat: Decimal
    lng: Decimal
}
",
    );
    let failed = plain(&root, &["package", "check"]);
    assert_eq!(failed.code, 1, "{}", failed.stdout);
    assert!(
        failed.stdout.starts_with(
            "error: `GeoPoint` is the standard `Point` under another name
 --> types/geo_point.jadpo:2:13
  |
2 | public type GeoPoint = Object {
  |             ^^^^^^^^ a latitude and a longitude
  reason: `GeoPoint` has only a latitude (`lat`) and a longitude (`lng`), which is the standard `Point`. Packages that each declare their own disagree on order and on names, and an app has to convert between them.
  next: Use `Point` in the package's public types, converting at the edge with `geo.from_lat_lng` or `geo.from_lng_lat` where the provider sends a pair, or write `decline type.standard_exists: \"<why>\"` above it if this is deliberate (agent)
  rule: type.standard_exists
  help: docs/language/19-standard-library.html#type-standard-exists
"
        ),
        "{}",
        failed.stdout
    );
    assert!(
        failed.stdout.contains(
            "error: Public type `OutwardCode` has no description
 --> types/outward_code.jadpo:1:13
"
        ),
        "{}",
        failed.stdout
    );
    assert!(
        failed.stdout.contains(
            "Publishing bar
  compiles      failed                        1 error
  described     failed                        1 has no description
"
        ),
        "{}",
        failed.stdout
    );
    assert!(
        failed
            .stdout
            .ends_with("package check failed: 2 failed, 1 passed, 4 not checked yet\n"),
        "{}",
        failed.stdout
    );
    assert!(!root.join("surface.jadpo_record").exists());
}

#[test]
fn check_needs_a_package() {
    let dir = tempdir::Dir::new("package-check-none");
    let refused = plain(dir.path(), &["package", "check"]);
    assert_eq!(refused.code, 2);
    assert!(
        refused
            .stderr
            .starts_with("error: There is no package here\n"),
        "{}",
        refused.stderr
    );
}
