//! What a release shows (area 11, "What a release shows", "The changelog"
//! and "The verifier"; chunk R3): `jadpo package check` prints a release's
//! facts and change record above the bar, the previous release's examples
//! and tests run against the new code, and `jadpo package verify --json`
//! writes everything the registry shows, on a commit it never changes.

use std::path::Path;

use jadpo_registry::shape::{Shape, check_value};

use super::http::{tempdir, write};
use super::packages::{UK_EXAMPLE, git, plain, repository};

/// `data` of `jadpo package <args> --json`.
fn json(cwd: &Path, args: &[&str]) -> (serde_json::Value, serde_json::Value, u8) {
    let mut args: Vec<&str> = args.to_vec();
    args.push("--json");
    let output = plain(cwd, &args);
    let document: serde_json::Value = serde_json::from_str(&output.stdout)
        .unwrap_or_else(|_| panic!("JSON: {}{}", output.stdout, output.stderr));
    (document["data"].clone(), document, output.code)
}

/// The package's first release, tagged.
fn released(name: &str) -> (tempdir::Dir, std::path::PathBuf) {
    let (dir, root) = repository(name);
    plain(&root, &["package", "check"]);
    let log = std::fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog");
    write(
        &root,
        "CHANGELOG.md",
        &format!("{log}\nReads UK postcodes as people type them.\n"),
    );
    let checked = plain(&root, &["package", "check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "first release"]);
    git(&root, &["tag", "v2.0.0"]);
    (dir, root)
}

/// `uk.parse` answering in lower case: the same types, other behaviour.
fn lower_case() -> String {
    UK_EXAMPLE.replace("text.upper(written)", "text.lower(written)")
}

const OUTWARD: &str = "/// The first half of a postcode, such as \"SW1A\".
public type OutwardCode = Text {
    max_length: 4
}
";

/// A provider package: one public operation that sends personal data, one
/// an action of its own keeps to itself.
const PAY: &[(&str, &str)] = &[
    (
        "package.jadpo",
        "/// Acme Pay's payments API.\npackage AcmePay.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    ),
    (
        "provider.jadpo",
        "/// Acme Pay's payments API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n}\n",
    ),
    (
        "types/customer_id.jadpo",
        "/// Acme Pay's id for a customer.\npublic type CustomerId = Text {\n    max_length: 40\n}\n",
    ),
    (
        "types/customer_request.jadpo",
        "/// A customer to create.\npublic type CustomerRequest = Object {\n    /// Their account's own notes.\n    notes: Text {\n        personal: PersonalData.health\n    }\n}\n",
    ),
    (
        "types/customer_reply.jadpo",
        "/// The customer Acme Pay made.\npublic type CustomerReply = Object {\n    /// Acme Pay's id for them.\n    id: CustomerId\n}\n",
    ),
    (
        "resources/customers.jadpo",
        "/// Customers.\npublic resource customers {\n    /// Creates a customer.\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/customers\"\n        input: CustomerRequest\n        output: CustomerReply\n        changes: true\n        idempotent: true\n        source: PackageSource.written\n    }\n\n    /// Reads a customer.\n    get: {\n        method: HttpMethod.get\n        path: \"/v1/customers\"\n        output: CustomerReply\n        source: PackageSource.written\n    }\n}\n",
    ),
];

fn pay_repository(name: &str) -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new(name);
    let root = dir.path().join("acme_pay");
    for (path, text) in PAY {
        write(&root, path, text);
    }
    write(
        &root,
        "recordings/customers.get.json",
        "{\"id\": \"{Text}\"}\n",
    );
    git(&root, &["init", "-q", "-b", "main"]);
    (dir, root)
}

// --- check prints the facts ------------------------------------------------

#[test]
fn check_prints_a_librarys_facts_above_the_bar() {
    let (_dir, root) = repository("release-library-facts");
    let checked = plain(&root, &["package", "check"]);
    assert!(
        checked.stdout.contains(
            "Facts
  kind: library, pure
  storable: UkPostcode, WrittenPostcode
  failures: uk.parse: InvalidPostcode

Publishing bar
"
        ),
        "{}",
        checked.stdout
    );
    let (data, _, _) = json(&root, &["package", "check"]);
    assert_eq!(data["facts"]["kind"], "library");
    assert_eq!(data["facts"]["pure"], true);
    assert_eq!(data["change_record"], serde_json::json!([]));
    assert_eq!(data["previous"], serde_json::Value::Null);
}

#[test]
fn check_prints_a_providers_facts_and_what_it_sends() {
    let (_dir, root) = pay_repository("release-provider-facts");
    let checked = plain(&root, &["package", "check"]);
    assert!(
        checked.stdout.contains(
            "Facts
  kind: provider
  hosts: https://api.acme-pay.example
  changes: customers.create
  erasure: not declared
  storable: CustomerId
  failures: none
  contract: 1 of 2 operations confirmed

Personal data sent
  to https://api.acme-pay.example
  customers.create  CustomerRequest.notes  health, sensitive

Publishing bar
"
        ),
        "{}",
        checked.stdout
    );
}

#[test]
fn a_package_with_errors_shows_no_facts() {
    let (_dir, root) = repository("release-errors");
    write(
        &root,
        "types/uk_postcode.jadpo",
        "public type UkPostcode = Nothing\n",
    );
    let checked = plain(&root, &["package", "check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(!checked.stdout.contains("Facts\n"), "{}", checked.stdout);
    let (data, _, _) = json(&root, &["package", "check"]);
    assert_eq!(data["facts"], serde_json::Value::Null);
}

#[test]
fn show_gives_no_facts_for_an_own_package_with_errors() {
    let dir = tempdir::Dir::new("release-show-errors");
    let app = dir.path().join("shop");
    write(&app, "app.jadpo", "application {}\n");
    for (path, text) in super::packages::package_files() {
        write(&app, &format!("shared/providers/postcodes/v2/{path}"), text);
    }
    write(
        &app,
        "shared/providers/postcodes/v2/types/uk_postcode.jadpo",
        "/// A UK postcode.\npublic type UkPostcode = Nothing\n",
    );
    let shown = plain(&app, &["package", "show", "postcodes"]);
    assert_eq!(shown.code, 0, "{}{}", shown.stdout, shown.stderr);
    assert!(!shown.stdout.contains("Facts\n"), "{}", shown.stdout);
    let (data, _, _) = json(&app, &["package", "show", "postcodes"]);
    assert_eq!(data["facts"], serde_json::Value::Null);
}

#[test]
fn show_gives_the_same_facts() {
    let (_dir, root) = repository("release-show");
    let (checked, _, _) = json(&root, &["package", "check"]);
    let dir = tempdir::Dir::new("release-show-app");
    let app = dir.path().join("shop");
    write(&app, "app.jadpo", "application {}\n");
    for (path, text) in super::packages::package_files() {
        write(&app, &format!("shared/providers/postcodes/v2/{path}"), text);
    }
    let shown = plain(&app, &["package", "show", "postcodes"]);
    assert_eq!(shown.code, 0, "{}{}", shown.stdout, shown.stderr);
    assert!(
        shown.stdout.contains(
            "Facts
  kind: library, pure
  storable: UkPostcode, WrittenPostcode
  failures: uk.parse: InvalidPostcode
"
        ),
        "{}",
        shown.stdout
    );
    let (data, _, _) = json(&app, &["package", "show", "postcodes"]);
    assert_eq!(data["facts"], checked["facts"]);
}

// --- the change record -----------------------------------------------------

#[test]
fn the_change_record_is_printed_and_written_to_the_changelog() {
    let (_dir, root) = released("release-record");
    write(&root, "types/outward_code.jadpo", OUTWARD);
    let checked = plain(&root, &["package", "check"]);
    assert!(
        checked.stdout.contains(
            "Changes since 2.0.0
  + added  type OutwardCode

Publishing bar
"
        ),
        "{}",
        checked.stdout
    );
    assert!(
        checked
            .stdout
            .contains("  behaviour     passed                        all 2 from 2.0.0 hold\n"),
        "{}",
        checked.stdout
    );
    let log = std::fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog");
    assert!(
        log.starts_with("# Changelog\n\n## 2.1.0\n\n- added type `OutwardCode`\n"),
        "{log}"
    );
    let (data, _, _) = json(&root, &["package", "check"]);
    let entry = &data["change_record"][0];
    assert_eq!(entry["declaration"], "OutwardCode");
    assert_eq!(entry["kind"], "type");
    assert_eq!(entry["change"], "added");
    assert_eq!(data["previous"]["since"], "2.0.0");
    assert_eq!(data["previous"]["ran"], 2);
}

#[test]
fn a_tagged_release_has_nothing_to_compare() {
    let (_dir, root) = released("release-tagged");
    let checked = plain(&root, &["package", "check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    assert!(
        !checked.stdout.contains("Changes since"),
        "{}",
        checked.stdout
    );
    assert!(!checked.stdout.contains("behaviour"), "{}", checked.stdout);
}

#[test]
fn a_new_host_and_personal_data_sent_there_are_in_the_record() {
    let (_dir, root) = pay_repository("release-new-host");
    plain(&root, &["package", "check"]);
    let log = std::fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog");
    write(
        &root,
        "CHANGELOG.md",
        &format!("{log}\nThe first release.\n"),
    );
    assert_eq!(plain(&root, &["package", "check"]).code, 0);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "first"]);
    git(&root, &["tag", "v1.0.0"]);
    write(
        &root,
        "provider.jadpo",
        "/// Acme Pay's payments API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n}\n",
    );
    let checked = plain(&root, &["package", "check"]);
    assert!(
        checked.stdout.contains(
            "Changes since 1.0.0
  + new host       provider          https://sandbox.acme-pay.example
  + personal data  customers.create
    CustomerRequest.notes to https://sandbox.acme-pay.example

Publishing bar
"
        ),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains("1.1.0, 1 new host since 1.0.0"),
        "{}",
        checked.stdout
    );
    let log = std::fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog");
    assert!(
        log.contains(
            "- new host `https://sandbox.acme-pay.example`\n- personal data sent by operation `customers.create`: CustomerRequest.notes to https://sandbox.acme-pay.example\n"
        ),
        "{log}"
    );
}

// --- the previous release's examples and tests ------------------------------

#[test]
fn behaviour_the_types_dont_show_is_refused_within_a_major_version() {
    let (_dir, root) = released("release-behaviour");
    write(&root, "libraries/uk.jadpo", &lower_case());
    // The release's own tests changed with the code: only the last
    // release's hold it to what it did.
    write(
        &root,
        "libraries/uk.test.jadpo",
        "test \"a postcode is read\" {\n    var postcode: UkPostcode = attempt uk.parse(\"sw1a 1aa\")\n    assert postcode == \"sw1a 1aa\"\n    assert uk.parse(\"x\") fails InvalidPostcode\n}\n",
    );
    let mut lower = lower_case();
    lower = lower.replace(
        "assert postcode == \"SW1A 1AA\"",
        "assert postcode == \"sw1a 1aa\"",
    );
    write(&root, "libraries/uk.jadpo", &lower);
    let checked = plain(&root, &["package", "check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked.stdout.contains(
            "error: `uk.parse` behaves differently from 2.0.0
  reason: `a postcode is read in upper case`, from 2.0.0, now fails against the new code."
        ),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains("rule: publish.behaviour_changed"),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains(
            "next: Put back what it did, publish the change as `Postcodes.v3`, or, if 2.0.0 was wrong, mark it a fix under `## 2.0.1` in CHANGELOG.md: Fix `uk.parse`: why it had to change (human)"
        ),
        "{}",
        checked.stdout
    );
    assert!(
        checked
            .stdout
            .contains("  ~ behaviour changed  uk.parse  a postcode is read in upper case\n"),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains(
            "  behaviour     failed                        2 of 2 fail, not marked a fix\n"
        ),
        "{}",
        checked.stdout
    );

    // Marked a fix, with why, it passes, and the record carries the why.
    let log = std::fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog");
    assert!(
        log.contains("- behaviour changed `uk.parse`: a postcode is read in upper case\n"),
        "{log}"
    );
    write(
        &root,
        "CHANGELOG.md",
        &log.replacen(
            "\n## 2.0.0",
            "\nFix `uk.parse`: upper case broke the courier's lookup.\n\n## 2.0.0",
            1,
        ),
    );
    let fixed = plain(&root, &["package", "check"]);
    assert_eq!(fixed.code, 0, "{}", fixed.stdout);
    assert!(
        fixed
            .stdout
            .contains("  behaviour     passed                        2 of 2 fail, marked a fix\n"),
        "{}",
        fixed.stdout
    );
    assert!(
        fixed.stdout.contains(", marked a fix\n\nPublishing bar\n"),
        "{}",
        fixed.stdout
    );
    let (data, _, _) = json(&root, &["package", "check"]);
    let entries = data["change_record"].as_array().expect("a list");
    assert!(
        entries
            .iter()
            .all(|entry| entry["why"] == "upper case broke the courier's lookup."),
        "{entries:?}"
    );
}

#[test]
fn a_previous_test_that_no_longer_compiles_has_failed() {
    let (_dir, root) = released("release-no-compile");
    // A private helper an old test leans on, then taken away, must not hide
    // what the old test would have shown.
    write(
        &root,
        "libraries/tidy.jadpo",
        "/// Tidying what people type.\nlibrary tidy {\n    /// Reads it in upper case.\n    function read(written: WrittenPostcode) fails InvalidPostcode -> UkPostcode {\n        return attempt UkPostcode(text.upper(written)) {\n            invalid: InvalidPostcode\n        }\n    }\n}\n",
    );
    write(
        &root,
        "libraries/uk.jadpo",
        &UK_EXAMPLE.replace(
            "return attempt UkPostcode(text.upper(written)) {\n            invalid: InvalidPostcode\n        }",
            "return attempt tidy.read(written)",
        ),
    );
    write(
        &root,
        "libraries/uk.test.jadpo",
        "test \"a tidied postcode reads\" {\n    assert tidy.read(\"x\") fails InvalidPostcode\n    assert uk.parse(\"x\") fails InvalidPostcode\n}\n",
    );
    plain(&root, &["package", "check"]);
    let log = std::fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog");
    write(
        &root,
        "CHANGELOG.md",
        &log.replacen("\n## 2.0.0", "\nAdds a helper.\n\n## 2.0.0", 1),
    );
    let second = plain(&root, &["package", "check"]);
    assert_eq!(second.code, 0, "{}", second.stdout);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "second"]);
    git(&root, &["tag", "v2.0.1"]);
    // The helper goes, and the test beside it changes to do without.
    std::fs::remove_file(root.join("libraries/tidy.jadpo")).expect("removed");
    write(&root, "libraries/uk.jadpo", UK_EXAMPLE);
    write(
        &root,
        "libraries/uk.test.jadpo",
        "test \"a postcode reads\" {\n    var postcode: UkPostcode = attempt uk.parse(\"sw1a 1aa\")\n    assert postcode == \"SW1A 1AA\"\n    assert uk.parse(\"x\") fails InvalidPostcode\n}\n",
    );
    let checked = plain(&root, &["package", "check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked.stdout.contains(
            "`a tidied postcode reads`, from 2.0.1, now fails against the new code. It no longer compiles:"
        ),
        "{}",
        checked.stdout
    );
    // It calls the public `uk.parse` too, which is what the entry names.
    assert!(
        checked
            .stdout
            .contains("~ behaviour changed  uk.parse  a tidied postcode reads\n"),
        "{}",
        checked.stdout
    );
}

#[test]
fn a_breaking_change_stops_the_behaviour_row_saying_anything_yet() {
    let (_dir, root) = released("release-breaking");
    write(
        &root,
        "types/written_postcode.jadpo",
        "/// A postcode as someone typed it.\npublic type WrittenPostcode = Text {\n    max_length: 30\n}\n",
    );
    write(&root, "libraries/uk.jadpo", &lower_case());
    let checked = plain(&root, &["package", "check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked.stdout.contains("rule: publish.breaking_change"),
        "{}",
        checked.stdout
    );
    assert!(
        !checked.stdout.contains("rule: publish.behaviour_changed"),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains(
            "  behaviour     not checked yet               waits for an allowed version\n"
        ),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains(
            "  ~ retyped            type WrittenPostcode  max_length: 30, was max_length: 20\n"
        ),
        "{}",
        checked.stdout
    );
}

#[test]
fn a_new_major_versions_first_release_shows_what_it_broke() {
    let (_dir, root) = released("release-new-major");
    write(
        &root,
        "package.jadpo",
        &super::packages::INDEX.replace("Postcodes.v2", "Postcodes.v3"),
    );
    write(&root, "libraries/uk.jadpo", &lower_case());
    let checked = plain(&root, &["package", "check"]);
    assert!(
        checked.stdout.contains("3.0.0, new major since 2.0.0"),
        "{}",
        checked.stdout
    );
    // Allowed: a new major version may change what it does.
    assert!(
        checked.stdout.contains("2 of 2 fail, a new major\n"),
        "{}",
        checked.stdout
    );
    assert!(
        !checked.stdout.contains("rule: publish.behaviour_changed"),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains("Changes since 2.0.0\n"),
        "{}",
        checked.stdout
    );
}

// --- jadpo package verify ---------------------------------------------------

/// The package as a commit carries it, with `surface.jadpo_record` and its
/// changelog written, in a folder of its own, and its first release beside
/// it.
fn two_releases(name: &str) -> (tempdir::Dir, std::path::PathBuf, std::path::PathBuf) {
    let (dir, root) = released(name);
    let first = dir.path().join("first");
    copy(&root, &first);
    write(&root, "types/outward_code.jadpo", OUTWARD);
    plain(&root, &["package", "check"]);
    let log = std::fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog");
    write(
        &root,
        "CHANGELOG.md",
        &log.replacen("\n## 2.0.0", "\nReads the outward code.\n\n## 2.0.0", 1),
    );
    assert_eq!(plain(&root, &["package", "check"]).code, 0);
    (dir, root, first)
}

fn copy(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).expect("a folder") {
        let entry = entry.expect("an entry");
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a type").is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::create_dir_all(to).expect("folders");
            std::fs::copy(entry.path(), target).expect("copied");
        }
    }
}

#[test]
fn verify_writes_everything_a_release_shows() {
    let (dir, root, first) = two_releases("release-verify");
    let (data, document, code) = json(
        dir.path(),
        &[
            "package",
            "verify",
            root.to_str().expect("a path"),
            "--previous",
            first.to_str().expect("a path"),
        ],
    );
    assert_eq!(code, 0, "{document}");
    assert_eq!(data["package"], "Postcodes.v2");
    assert_eq!(data["version"], "2.1.0");
    assert_eq!(data["passed"], true);
    assert_eq!(data["jadpo"], env!("CARGO_PKG_VERSION"));
    let surface = data["surface"].as_str().expect("the surface");
    assert_eq!(
        surface,
        std::fs::read_to_string(root.join("surface.jadpo_record")).expect("surface")
    );
    let rows: Vec<&str> = data["bar"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["item"].as_str().expect("an item"))
        .collect();
    assert_eq!(
        rows,
        [
            "compiles",
            "described",
            "examples",
            "tests",
            "behaviour",
            "version",
            "changelog",
            "surface",
            "licence"
        ]
    );
    assert_eq!(data["previous"]["ran"], 2);
    assert_eq!(data["change_record"][0]["declaration"], "OutwardCode");
    // What the registry's protocol names them, field for field.
    check_value(&Shape::Named("Facts"), &data["facts"]).expect("facts");
    for entry in data["change_record"].as_array().expect("a list") {
        check_value(&Shape::Named("ChangeEntry"), entry).expect("an entry");
    }
    let parse = data["fingerprints"]
        .as_array()
        .expect("fingerprints")
        .iter()
        .find(|print| print["declaration"] == "uk.parse")
        .expect("uk.parse");
    assert!(
        parse["source"]
            .as_str()
            .expect("a digest")
            .starts_with("sha256:"),
        "{parse}"
    );
    assert_eq!(parse["shape"]["inputs"][0], "Text { max_length: 20 }");
    assert_eq!(parse["shape"]["failures"][0], "invalid_value");
    assert_eq!(parse["shape"]["effects"], serde_json::json!([]));
}

#[test]
fn verify_prints_the_same_screen_and_writes_nothing() {
    let (dir, root, first) = two_releases("release-verify-screen");
    let surface = std::fs::read(root.join("surface.jadpo_record")).expect("surface");
    let log = std::fs::read(root.join("CHANGELOG.md")).expect("changelog");
    let verified = plain(
        dir.path(),
        &[
            "package",
            "verify",
            root.to_str().expect("a path"),
            "--previous",
            first.to_str().expect("a path"),
        ],
    );
    assert_eq!(verified.code, 0, "{}", verified.stdout);
    assert!(verified.stdout.contains("Facts\n"), "{}", verified.stdout);
    assert!(
        verified.stdout.contains("Changes since 2.0.0\n"),
        "{}",
        verified.stdout
    );
    assert!(
        verified
            .stdout
            .contains("  surface       passed                        matches its code\n"),
        "{}",
        verified.stdout
    );
    assert!(
        verified
            .stdout
            .ends_with("package verify passed: Postcodes.v2 2.1.0, 9 passed, 5 fingerprints\n"),
        "{}",
        verified.stdout
    );
    assert!(!verified.stdout.contains("Wrote"), "{}", verified.stdout);
    assert_eq!(
        std::fs::read(root.join("surface.jadpo_record")).expect("surface"),
        surface
    );
    assert_eq!(
        std::fs::read(root.join("CHANGELOG.md")).expect("changelog"),
        log
    );
}

#[test]
fn verify_refuses_a_commit_whose_files_arent_what_its_code_gives() {
    let (dir, root, first) = two_releases("release-verify-stale");
    let surface = std::fs::read_to_string(root.join("surface.jadpo_record")).expect("surface");
    let log = std::fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog");
    let args = [
        "package",
        "verify",
        root.to_str().expect("a path"),
        "--previous",
        first.to_str().expect("a path"),
    ];
    write(
        &root,
        "surface.jadpo_record",
        &surface.replace("kind library", "kind provider"),
    );
    let stale = plain(dir.path(), &args);
    assert_eq!(stale.code, 1, "{}", stale.stdout);
    assert!(
        stale
            .stdout
            .contains("error: `surface.jadpo_record` isn't what the package's code gives\n"),
        "{}",
        stale.stdout
    );
    assert!(
        stale.stdout.contains("rule: publish.stale_files"),
        "{}",
        stale.stdout
    );
    // It writes nothing, so the stale file stays for the author to fix.
    assert_ne!(
        std::fs::read_to_string(root.join("surface.jadpo_record")).expect("surface"),
        surface
    );
    write(&root, "surface.jadpo_record", &surface);
    write(
        &root,
        "CHANGELOG.md",
        &log.replace("- added type `OutwardCode`", "- nothing to see here"),
    );
    let listed = plain(dir.path(), &args);
    assert_eq!(listed.code, 1, "{}", listed.stdout);
    assert!(
        listed
            .stdout
            .contains("error: `CHANGELOG.md` isn't what the package's code gives\n"),
        "{}",
        listed.stdout
    );
}

#[test]
fn verify_without_a_previous_release_is_a_first_release() {
    let (dir, root) = released("release-verify-first");
    let (data, document, code) = json(
        dir.path(),
        &["package", "verify", root.to_str().expect("a path")],
    );
    assert_eq!(code, 0, "{document}");
    assert_eq!(data["version"], "2.0.0");
    assert_eq!(data["previous"], serde_json::Value::Null);
    assert_eq!(data["change_record"], serde_json::json!([]));
}

#[test]
fn verify_of_a_copy_gives_the_same_fingerprint_under_another_name() {
    let (dir, root) = released("release-verify-copy");
    let (data, _, _) = json(
        dir.path(),
        &["package", "verify", root.to_str().expect("a path")],
    );
    let print = |data: &serde_json::Value| {
        data["fingerprints"]
            .as_array()
            .expect("fingerprints")
            .iter()
            .find(|print| print["declaration"] == "uk.parse")
            .expect("uk.parse")["source"]
            .clone()
    };
    // Another package, carrying a reformatted copy with another variable.
    let copy_root = dir.path().join("copied");
    copy(&root, &copy_root);
    write(
        &copy_root,
        "package.jadpo",
        &super::packages::INDEX.replace("Postcodes.v2", "Addresses.v1"),
    );
    write(
        &copy_root,
        "libraries/uk.jadpo",
        &UK_EXAMPLE
            .replace("written: WrittenPostcode", "typed: WrittenPostcode")
            .replace("text.upper(written)", "text.upper(typed)"),
    );
    let (copied, _, _) = json(
        dir.path(),
        &["package", "verify", copy_root.to_str().expect("a path")],
    );
    assert_eq!(print(&copied), print(&data));
}

#[test]
fn verify_says_what_it_needs() {
    let dir = tempdir::Dir::new("release-verify-arguments");
    let none = plain(dir.path(), &["package", "verify"]);
    assert_eq!(none.code, 2, "{}", none.stderr);
    assert!(
        none.stderr
            .contains("`jadpo package verify` needs a package's folder"),
        "{}",
        none.stderr
    );
    let serve = plain(dir.path(), &["package", "verify", "--serve"]);
    assert_eq!(serve.code, 1, "{}", serve.stderr);
    assert!(
        serve
            .stderr
            .contains("`jadpo package verify --serve` isn't built yet"),
        "{}",
        serve.stderr
    );
    // Another package's release can't stand in as the one before it, which
    // would pass as a new major version.
    let (other, root) = released("release-verify-other");
    let copied = other.path().join("copied");
    copy(&root, &copied);
    write(
        &copied,
        "package.jadpo",
        &super::packages::INDEX.replace("Postcodes.v2", "Addresses.v1"),
    );
    let another = plain(
        dir.path(),
        &[
            "package",
            "verify",
            root.to_str().expect("a path"),
            "--previous",
            copied.to_str().expect("a path"),
        ],
    );
    assert_eq!(another.code, 2, "{}", another.stderr);
    assert!(
        another
            .stderr
            .contains("`Addresses.v1` isn't an earlier release of `Postcodes.v2`"),
        "{}",
        another.stderr
    );
    let (_kept, root) = repository("release-verify-previous");
    let refused = plain(
        dir.path(),
        &[
            "package",
            "verify",
            root.to_str().expect("a path"),
            "--previous",
            dir.path().to_str().expect("a path"),
        ],
    );
    assert_eq!(refused.code, 2, "{}", refused.stderr);
    assert!(
        refused.stderr.contains("isn't a released package"),
        "{}",
        refused.stderr
    );
}
