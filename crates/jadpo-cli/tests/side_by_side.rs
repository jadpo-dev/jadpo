//! Two versions of one package side by side, lazy loading and the package
//! cache (area 11, "Where packages live", "The package format"; area 18,
//! "The lock"; docs/compiler-runtime.md §2.3; chunk 7.1c2): `Postcodes.v2`
//! and `Postcodes.v3` installed and locked together, names carrying the
//! version, checked, run in the interpreter and moved across with the
//! `jadpo package` commands; an installed provider's resources compiled
//! only once the app calls into them; and an installed package's check
//! kept by its digest.

use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_db::{Locked, Source, package_digest, parse_lock, write_lock};
use jadpo_term::Environment;

use super::http::{jadpo, tempdir, write};
use super::packages::{plain, repository};

/// `v3` of the package: what an upgrade brings, reading a postcode with
/// spaces around it.
const UK_THREE: &str = "/// Reading UK postcodes.
public library uk {
    /// Reads a UK postcode as it is written, spaces around it left out.
    ///
    /// ```jadpo
    /// var postcode: UkPostcode = attempt uk.parse(\" sw1a 1aa \")
    /// assert postcode == \"SW1A 1AA\"
    /// assert uk.parse(\"x\") fails InvalidPostcode
    /// ```
    function parse(written: WrittenPostcode) fails InvalidPostcode -> UkPostcode {
        return attempt UkPostcode(text.upper(text.trim(written))) {
            invalid: InvalidPostcode
        }
    }
}
";

/// The app's action still on `Postcodes.v2`.
const OLD: &str = "use packages/postcodes

/// Reads the postcode a customer typed.
action check_postcode(
    written: Postcodes.v2.WrittenPostcode
) fails Postcodes.v2.InvalidPostcode -> Postcodes.v2.UkPostcode {
    return attempt Postcodes.v2.uk.parse(written)
}
";

const OLD_TEST: &str = "use packages/postcodes

test \"the old version reads a postcode as typed\" {
    var postcode: Postcodes.v2.UkPostcode = attempt check_postcode(\"sw1a 1aa\")
    assert postcode == \"SW1A 1AA\"
}

test \"the old version refuses spaces around a postcode\" {
    assert check_postcode(\" sw1a 1aa \") fails Postcodes.v2.InvalidPostcode
}
";

/// The app's action moved to `Postcodes.v3`.
const NEW: &str = "use packages/postcodes

/// Reads the postcode a customer typed, spaces and all.
action check_new_postcode(
    written: Postcodes.v3.WrittenPostcode
) fails Postcodes.v3.InvalidPostcode -> Postcodes.v3.UkPostcode {
    return attempt Postcodes.v3.uk.parse(written)
}
";

const NEW_TEST: &str = "use packages/postcodes

test \"the new version leaves out spaces around a postcode\" {
    var postcode: Postcodes.v3.UkPostcode = attempt check_new_postcode(\" sw1a 1aa \")
    assert postcode == \"SW1A 1AA\"
}
";

/// The package's repository at `v2` and a checkout of `v3` beside it, each
/// with its surface written, and an app with both installed from them:
/// `<dir>/postcodes`, `<dir>/postcodes-3` and `<dir>/shop`.
fn both(name: &str) -> (tempdir::Dir, PathBuf) {
    let (dir, two) = repository(name);
    plain(&two, &["package", "check"]);
    assert!(two.join("surface.jadpo_record").exists());
    let three = dir.path().join("postcodes-3");
    for (path, text) in super::packages::package_files() {
        write(&three, path, &text.replace("Postcodes.v2", "Postcodes.v3"));
    }
    write(&three, "libraries/uk.jadpo", UK_THREE);
    let checked = plain(&three, &["package", "check"]);
    assert!(
        three.join("surface.jadpo_record").exists(),
        "{}",
        checked.stdout
    );
    let app = dir.path().join("shop");
    write(&app, "app.jadpo", "application {}\n");
    let added = jadpo(&app, &["package", "add", "../postcodes"]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    let added = jadpo(&app, &["package", "add", "../postcodes-3"]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    (dir, app)
}

#[test]
fn a_second_version_installs_beside_the_first_and_both_run() {
    let (_dir, app) = both("side-by-side-run-7.1c2");
    let text = std::fs::read_to_string(app.join("jadpo.lock")).expect("the lock");
    let lines: Vec<String> = parse_lock(&text)
        .expect("a lock that reads")
        .into_iter()
        .map(|line| line.package)
        .collect();
    assert_eq!(lines, ["Postcodes.v2", "Postcodes.v3"]);
    write(&app, "domains/intake/actions/check_postcode.jadpo", OLD);
    write(
        &app,
        "domains/intake/actions/check_postcode.test.jadpo",
        OLD_TEST,
    );
    write(&app, "domains/intake/actions/check_new_postcode.jadpo", NEW);
    write(
        &app,
        "domains/intake/actions/check_new_postcode.test.jadpo",
        NEW_TEST,
    );
    let checked = jadpo(&app, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    // Each version's code runs in the interpreter, told apart.
    let tested = jadpo(&app, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("3 passed"), "{}", tested.stdout);
}

#[test]
fn the_package_commands_need_the_version_while_two_are_held() {
    let (_dir, app) = both("side-by-side-commands-7.1c2");
    let shown = jadpo(&app, &["package", "show", "postcodes"]);
    assert_eq!(shown.code, 2, "{}", shown.stdout);
    assert!(
        shown
            .stderr
            .starts_with("error: `postcodes` names 2 packages\n"),
        "{}",
        shown.stderr
    );
    assert!(
        shown.stderr.contains(
            "next: Use `jadpo package show Postcodes.v2` or `jadpo package show Postcodes.v3`"
        ),
        "{}",
        shown.stderr
    );
    let shown = jadpo(&app, &["package", "show", "Postcodes.v3"]);
    assert_eq!(shown.code, 0, "{}", shown.stderr);
    assert!(shown.stdout.contains("Postcodes.v3"), "{}", shown.stdout);
    let declaration = jadpo(&app, &["package", "show", "Postcodes.v3", "uk.parse"]);
    assert_eq!(declaration.code, 0, "{}", declaration.stderr);
    for command in ["update", "remove"] {
        let refused = jadpo(&app, &["package", command, "postcodes"]);
        assert_eq!(refused.code, 2, "{command}: {}", refused.stdout);
        assert!(
            refused
                .stderr
                .starts_with("error: `postcodes` names 2 packages\n"),
            "{command}: {}",
            refused.stderr
        );
    }
}

#[test]
fn callers_move_across_then_the_old_version_goes() {
    let (_dir, app) = both("side-by-side-move-7.1c2");
    write(&app, "domains/intake/actions/check_postcode.jadpo", OLD);
    write(&app, "domains/intake/actions/check_new_postcode.jadpo", NEW);
    // `v2` is still written through, so it stays.
    let refused = jadpo(&app, &["package", "remove", "Postcodes.v2"]);
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(
        refused.stderr.contains(
            "`domains/intake/actions/check_postcode.jadpo` writes names through `Postcodes.v2`"
        ),
        "{}",
        refused.stderr
    );
    // Its caller moves across, and `v2` goes with its lock line.
    std::fs::remove_file(app.join("domains/intake/actions/check_postcode.jadpo")).expect("remove");
    let removed = jadpo(&app, &["package", "remove", "Postcodes.v2"]);
    assert_eq!(removed.code, 0, "{}{}", removed.stdout, removed.stderr);
    assert!(!app.join("packages/postcodes/v2").exists());
    // With one version held, its names no longer carry it: each is
    // reported with the edit that drops it.
    let checked = jadpo(&app, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked
            .stdout
            .contains("error: `Postcodes` is held in one version, so its names don't carry it"),
        "{}",
        checked.stdout
    );
    // An agent applies each edit `jadpo check --json` carries.
    let report = jadpo(&app, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&report.stdout).expect("json");
    let path = "domains/intake/actions/check_new_postcode.jadpo";
    let mut edits: Vec<(usize, usize, String)> = document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .flat_map(|d| d["next"]["edits"].as_array().cloned().unwrap_or_default())
        .filter(|edit| edit["path"] == path)
        .map(|edit| {
            (
                edit["start"].as_u64().expect("start") as usize,
                edit["end"].as_u64().expect("end") as usize,
                edit["replacement"]
                    .as_str()
                    .expect("replacement")
                    .to_string(),
            )
        })
        .collect();
    assert_eq!(edits.len(), 4, "{}", report.stdout);
    edits.sort();
    let mut text = std::fs::read_to_string(app.join(path)).expect("read");
    for (start, end, replacement) in edits.into_iter().rev() {
        text.replace_range(start..end, &replacement);
    }
    assert_eq!(text, NEW.replace("Postcodes.v3.", "Postcodes."));
    write(&app, path, &text);
    let checked = jadpo(&app, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
}

// --- a package's enum values in two versions ------------------------------

/// The app's own `Postcodes` at `v2` and `v3`, each with a `Round` enum,
/// one caller on each, and a test of each that writes the enum's values
/// through the package and its version (chunk 7.1e).
#[test]
fn a_package_enum_s_values_carry_the_version_and_run() {
    let dir = tempdir::Dir::new("side-by-side-enum-7.1e");
    let app = dir.path();
    write(app, "app.jadpo", "application {}\n");
    for version in ["v2", "v3"] {
        let folder = format!("shared/providers/postcodes/{version}");
        write(
            app,
            &format!("{folder}/package.jadpo"),
            &format!(
                "/// UK postcodes' collection rounds.\npackage Postcodes.{version} {{\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}}\n"
            ),
        );
        write(
            app,
            &format!("{folder}/types/round.jadpo"),
            "/// When a postcode's collections run.\npublic type Round = Enum {\n    morning\n    evening\n    /// Collected with another round.\n    shared: {\n        with: RoundName\n    }\n}\n",
        );
        write(
            app,
            &format!("{folder}/types/round_name.jadpo"),
            "/// A round's name.\npublic type RoundName = Text {\n    max_length: 20\n}\n",
        );
        let action = format!("later_round_{version}");
        write(
            app,
            &format!("domains/rounds/actions/{action}.jadpo"),
            &format!(
                "use shared/providers/postcodes\n\n/// The round after `round`.\naction {action}(round: Postcodes.{version}.Round) -> Postcodes.{version}.Round {{\n    return match round {{\n        Postcodes.{version}.Round.morning -> Postcodes.{version}.Round.evening\n        Postcodes.{version}.Round.evening -> Postcodes.{version}.Round.shared {{\n            with: \"late\"\n        }}\n        Postcodes.{version}.Round.shared -> round\n    }}\n}}\n"
            ),
        );
        write(
            app,
            &format!("domains/rounds/actions/{action}.test.jadpo"),
            &format!(
                "use shared/providers/postcodes\n\ntest \"{version}'s round moves on\" {{\n    assert {action}(Postcodes.{version}.Round.morning) == Postcodes.{version}.Round.evening\n    assert {action}(Postcodes.{version}.Round.evening) == Postcodes.{version}.Round.shared {{\n        with: \"late\"\n    }}\n}}\n"
            ),
        );
    }
    let checked = jadpo(app, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    let tested = jadpo(app, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("2 passed"), "{}", tested.stdout);
}

// --- lazy loading ------------------------------------------------------------

/// An installed provider package with two resources, one of which
/// doesn't parse.
fn couriers() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "package.jadpo",
            "/// Acme Couriers' booking API.\npackage AcmeCouriers.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
        ),
        (
            "provider.jadpo",
            "/// Acme Couriers' booking and tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: \"https://api.acme-couriers.example\"
        sandbox: \"https://sandbox.acme-couriers.example\"
    }
    credential: CredentialScheme.bearer
}
",
        ),
        (
            "resources/bookings.jadpo",
            "public resource bookings {
    create: {
        method: HttpMethod.post
        path: \"/v1/bookings\"
        input: CreateBooking
        output: Booking
        changes: true
        idempotent: true
        source: PackageSource.written
    }
}
",
        ),
        (
            "resources/tracking.jadpo",
            "public resource tracking {
    get: {
        method: HttpMethod.get
        path: \"/v1/tracking/{id}\"
        input: Booking
        output: Booking
        changes: false
        source: PackageSource.written
",
        ),
        (
            "types/create_booking.jadpo",
            "/// What booking a collection takes.\npublic type CreateBooking = Object {\n    postcode: Text\n    slot: Text\n}\n",
        ),
        (
            "types/booking.jadpo",
            "/// A booked collection.\npublic type Booking = Object {\n    id: Text\n}\n",
        ),
    ]
}

/// An app connected to the installed couriers package, locked by its
/// digest.
fn connected(name: &str) -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new(name);
    let app = dir.path().join("shop");
    write(
        &app,
        "app.jadpo",
        "application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
        }
    }
}
",
    );
    write(
        &app,
        "config.jadpo",
        "config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
}
",
    );
    write(
        &app,
        "domains/shipping/types/slot.jadpo",
        "/// A collection slot.\ntype Slot = Text {\n    max_length: 20\n}\n",
    );
    write(
        &app,
        "domains/shipping/actions/book.jadpo",
        "use shipping/types { Slot }\n\n/// Books a collection.\naction book(slot: Slot) -> Slot {\n    return slot\n}\n",
    );
    for (path, text) in couriers() {
        write(&app, &format!("packages/acme_couriers/v1/{path}"), text);
    }
    lock(&app, &couriers());
    (dir, app)
}

fn lock(app: &Path, files: &[(&str, &str)]) {
    let held: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(path, text)| (*path, text.as_bytes()))
        .collect();
    write(
        app,
        "jadpo.lock",
        &write_lock(&[Locked {
            package: "AcmeCouriers.v1".to_string(),
            source: Source::Path {
                version: "1.0.0".to_string(),
                path: "../acme-couriers".to_string(),
                digest: package_digest(&held),
            },
        }]),
    );
}

/// The project's source files and its lazily held ones, by path.
fn read(app: &Path) -> (Vec<String>, Vec<String>) {
    let loaded = jadpo_db::load(app);
    let paths = |files: &[jadpo_db::File]| {
        files
            .iter()
            .map(|file| file.path(&loaded.db).clone())
            .filter(|path| path.contains("/resources/"))
            .collect()
    };
    (
        paths(loaded.project.files(&loaded.db)),
        paths(loaded.project.lazy(&loaded.db)),
    )
}

const TRACKING: &str = "packages/acme_couriers/v1/resources/tracking.jadpo";
const BOOKINGS: &str = "packages/acme_couriers/v1/resources/bookings.jadpo";

#[test]
fn an_installed_provider_s_resources_wait_until_the_app_calls_into_them() {
    let (_dir, app) = connected("lazy-resources-7.1c2");
    assert_eq!(
        read(&app),
        (Vec::new(), vec![BOOKINGS.to_string(), TRACKING.to_string()])
    );
    // Neither resource is compiled, so the mistake in one isn't reached.
    let checked = jadpo(&app, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    // A call into one compiles that resource alone, found by its name.
    write(
        &app,
        "domains/shipping/actions/track.jadpo",
        "use shipping/types { Slot }\n\n/// Tracks a collection.\naction track(slot: Slot) -> Slot {\n    var parcel: Slot = couriers.tracking.get(slot)\n    return slot\n}\n",
    );
    assert_eq!(
        read(&app),
        (vec![TRACKING.to_string()], vec![BOOKINGS.to_string()])
    );
    let checked = jadpo(&app, &["check"]);
    assert!(
        checked
            .stdout
            .contains(" --> packages/acme_couriers/v1/resources/tracking.jadpo"),
        "{}",
        checked.stdout
    );
}

#[test]
fn a_resource_not_yet_compiled_still_counts_towards_the_digest() {
    let (_dir, app) = connected("lazy-digest-7.1c2");
    write(
        &app,
        "packages/acme_couriers/v1/resources/tracking.jadpo",
        "public resource tracking {}\n",
    );
    let checked = jadpo(&app, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked
            .stdout
            .contains("error: Package `AcmeCouriers.v1` differs from what was installed"),
        "{}",
        checked.stdout
    );
}

// --- the package cache -------------------------------------------------------

fn cached(cwd: &Path, cache: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", "1792504800")
        .with_var("XDG_CACHE_HOME", &cache.to_string_lossy())
        .with_cwd(cwd);
    run(&args, &env)
}

#[test]
fn an_installed_package_s_check_is_kept_by_its_digest() {
    let dir = tempdir::Dir::new("package-cache-7.1c2");
    let app = dir.path().join("shop");
    let cache = dir.path().join("cache");
    write(&app, "app.jadpo", "application {}\n");
    let mut files = super::packages::package_files();
    let broken = super::packages::UK.replace("text.upper(written)", "text.uppr(written)");
    for (path, text) in &mut files {
        if *path == "libraries/uk.jadpo" {
            *text = Box::leak(broken.clone().into_boxed_str());
        }
    }
    for (path, text) in &files {
        write(&app, &format!("packages/postcodes/v2/{path}"), text);
    }
    let held: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(path, text)| (*path, text.as_bytes()))
        .collect();
    let digest = package_digest(&held);
    write(
        &app,
        "jadpo.lock",
        &write_lock(&[Locked {
            package: "Postcodes.v2".to_string(),
            source: Source::Path {
                version: "2.1.0".to_string(),
                path: "../postcodes".to_string(),
                digest: digest.clone(),
            },
        }]),
    );
    let first = cached(&app, &cache, &["check"]);
    assert_eq!(first.code, 1, "{}", first.stdout);
    assert!(
        first
            .stdout
            .contains("packages/postcodes/v2/libraries/uk.jadpo")
    );
    // The files checked cleanly are kept under the digest; the one with a
    // problem isn't.
    let folder = cache
        .join("jadpo/packages")
        .join(digest.trim_start_matches("sha256:"));
    let entries: Vec<PathBuf> = std::fs::read_dir(&folder)
        .expect("the package's entry")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    assert_eq!(entries.len(), 1, "{entries:?}");
    let kept = std::fs::read_to_string(&entries[0]).expect("read");
    assert_eq!(
        kept,
        "failures/invalid_postcode.jadpo\npackage.jadpo\ntypes/uk_postcode.jadpo\ntypes/written_postcode.jadpo\n"
    );
    // A file the cache holds is not checked again.
    std::fs::write(&entries[0], format!("{kept}libraries/uk.jadpo\n")).expect("write");
    let second = cached(&app, &cache, &["check"]);
    assert_eq!(second.code, 0, "{}", second.stdout);
    // Without a cache, every file is checked.
    let plain = jadpo(&app, &["check"]);
    assert_eq!(plain.code, 1, "{}", plain.stdout);
}
