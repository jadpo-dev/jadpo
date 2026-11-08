//! The registry commands (area 11, "Installing and updating from the
//! registry", "Developing for the registry"; area 14, the package commands;
//! chunk R5), each run against a local registry on this machine or a stand-in
//! that misbehaves in front of it. Nothing here reaches another machine.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use jadpo_cli::{Output, run};
use jadpo_db::{Source, parse_lock};
use jadpo_registry::http::{Fetched, Incoming, Server, fetch};
use jadpo_registry::local::{LocalRegistry, ReleaseSeed, Served};
use jadpo_registry::names::{Handle, ScopedName, Version};
use jadpo_registry::protocol::{Advisory, Change, ChangeEntry, Method, ReleaseState};
use jadpo_term::Environment;

use super::http::{tempdir, write};
use super::installing::{release, workspace};
use super::packages::{git, plain};

const OUTWARD: &str = "/// The first half of a postcode, such as \"SW1A\".
public type OutwardCode = Text {
    max_length: 4
}
";

const USES: &str = "use packages/postcodes

/// Reads the postcode a customer typed.
action check_postcode(written: Postcodes.WrittenPostcode) fails Postcodes.InvalidPostcode -> Postcodes.UkPostcode {
    return attempt Postcodes.uk.parse(written)
}
";

fn name(text: &str) -> ScopedName {
    ScopedName::parse(text).unwrap()
}

fn version(text: &str) -> Version {
    Version::parse(text).unwrap()
}

/// RFC 3339 for `days` days ago, from the clock.
fn days_ago(days: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let at = now - days * 86_400;
    let (day, rest) = (at.div_euclid(86_400), at.rem_euclid(86_400));
    // Howard Hinnant's civil_from_days.
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// `jadpo <args>` with the local registry as its registry.
fn with(url: &str, cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(
        &args,
        &Environment::empty()
            .with_var("JADPO_REGISTRY", url)
            .with_cwd(cwd),
    )
}

fn json(url: &str, cwd: &Path, args: &[&str]) -> (serde_json::Value, u8) {
    let mut args = args.to_vec();
    args.push("--json");
    let output = with(url, cwd, &args);
    let document: serde_json::Value = serde_json::from_str(&output.stdout)
        .unwrap_or_else(|_| panic!("JSON: {}{}", output.stdout, output.stderr));
    (document, output.code)
}

/// The package's files as a release holds them.
fn snapshot(folder: &Path) -> Vec<(String, Vec<u8>)> {
    jadpo_db::package_files(folder)
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(folder)
                .unwrap()
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            (relative, std::fs::read(&path).unwrap())
        })
        .collect()
}

struct World {
    _dir: tempdir::Dir,
    package: PathBuf,
    app: PathBuf,
    registry: LocalRegistry,
    served: Served,
    url: String,
}

/// A package's 2.0.0 in a local registry, and an app beside it.
fn world(tag: &str, released: &str) -> World {
    let (dir, package, app) = workspace(tag);
    let registry = LocalRegistry::new();
    registry.publisher("@acme", true, false);
    seed(&registry, &package, "2.0.0", released, Vec::new());
    let served = registry.serve().expect("serves");
    let url = served.url();
    World {
        _dir: dir,
        package,
        app,
        registry,
        served,
        url,
    }
}

fn seed(
    registry: &LocalRegistry,
    folder: &Path,
    at: &str,
    released: &str,
    change_record: Vec<ChangeEntry>,
) {
    let mut seed = ReleaseSeed::new(name("@acme/postcodes"), version(at), snapshot(folder));
    seed.released_at = released.to_string();
    seed.surface = std::fs::read_to_string(folder.join("surface.jadpo_record")).expect("surface");
    seed.change_record = change_record;
    registry.release(seed);
}

/// The package moved to 2.1.0 with a new type, in the registry at `released`.
fn second_release(world: &World, released: &str) {
    release(&world.package, "v2.0.0");
    write(&world.package, "types/outward_code.jadpo", OUTWARD);
    let checked = plain(&world.package, &["package", "check"]);
    assert!(
        std::fs::read_to_string(world.package.join("surface.jadpo_record"))
            .expect("surface")
            .contains("version 2.1.0"),
        "{}",
        checked.stdout
    );
    seed(
        &world.registry,
        &world.package,
        "2.1.0",
        released,
        vec![ChangeEntry {
            declaration: "OutwardCode".into(),
            kind: "type".into(),
            change: Change::Added,
            detail: None,
            why: None,
        }],
    );
}

fn lock(app: &Path) -> Vec<jadpo_db::Locked> {
    parse_lock(&std::fs::read_to_string(app.join("jadpo.lock")).expect("the lock"))
        .expect("a lock that reads")
}

/// A stand-in in front of `registry`: `change` may rewrite each answer, and
/// every target asked for is kept.
fn stand_in(
    registry: &LocalRegistry,
    change: impl Fn(&Incoming, Fetched) -> Fetched + Send + Sync + 'static,
) -> (Server, String, Arc<Mutex<Vec<String>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let kept = asked.clone();
    let registry = registry.clone();
    let server = Server::start(Arc::new(move |incoming: Incoming| {
        kept.lock().unwrap().push(incoming.target.clone());
        change(&incoming, registry.handle(&incoming))
    }))
    .expect("serves");
    let url = format!("http://{}/registry/v1/", server.address());
    (server, url, asked)
}

// --- add -------------------------------------------------------------------

#[test]
fn add_installs_that_exact_name_and_says_who_it_comes_from() {
    let world = world("registry-add", &days_ago(40));
    let added = with(
        &world.url,
        &world.app,
        &["package", "add", "@acme/postcodes"],
    );
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    assert!(
        added.stdout.starts_with("Added Postcodes.v2 2.0.0\n"),
        "{}",
        added.stdout
    );
    for line in [
        "source     registry @acme/postcodes\n",
        "publisher  @acme, verified\n",
        "age        first released ",
        ", 40 days ago\n",
        "log        entry 1 of 1\n",
        "surface    matches its code\n",
        "kind:",
        "failures:",
    ] {
        assert!(added.stdout.contains(line), "{line}: {}", added.stdout);
    }
    match &lock(&world.app)[0].source {
        Source::Registry { version, name, .. } => {
            assert_eq!(version, "2.0.0");
            assert_eq!(name, "@acme/postcodes");
        }
        other => panic!("{other:?}"),
    }
    let lock_text = std::fs::read_to_string(world.app.join("jadpo.lock")).unwrap();
    assert!(
        lock_text
            .contains("package Postcodes.v2 version 2.0.0 registry @acme/postcodes digest sha256:"),
        "{lock_text}"
    );
    assert!(
        world
            .app
            .join("packages/postcodes/v2/package.jadpo")
            .is_file()
    );
    // The head it saw is kept, for the next add to prove against.
    let head = std::fs::read_to_string(world.app.join(".jadpo/registry_head")).expect("the head");
    assert!(head.starts_with("1 "), "{head}");
    // The app compiles against it, with no network at all.
    assert_eq!(plain(&world.app, &["check"]).code, 0);

    let shown = plain(&world.app, &["package", "show", "postcodes"]);
    assert_eq!(shown.code, 0, "{}", shown.stderr);
    assert!(
        shown.stdout.contains("registry @acme/postcodes"),
        "{}",
        shown.stdout
    );
    // The hint names it as it was added, and `show` takes that name
    // (Richard, 2026-11-07).
    assert!(
        added
            .stdout
            .ends_with("next: jadpo package show @acme/postcodes  shows what it offers\n"),
        "{}",
        added.stdout
    );
    let by_registry = plain(&world.app, &["package", "show", "@acme/postcodes"]);
    assert_eq!(by_registry.code, 0, "{}", by_registry.stderr);
    assert_eq!(by_registry.stdout, shown.stdout);
}

#[test]
fn add_stops_at_a_name_the_registry_doesnt_have() {
    let world = world("registry-add-unknown", &days_ago(40));
    // One letter out from a package it does have: never a near match.
    let near = with(
        &world.url,
        &world.app,
        &["package", "add", "@acme/postcode"],
    );
    assert_eq!(near.code, 1, "{}", near.stderr);
    assert!(
        near.stderr
            .starts_with("error: The registry has no package `@acme/postcode`\n"),
        "{}",
        near.stderr
    );
    assert!(
        near.stderr.contains("rule: install.not_in_registry\n"),
        "{}",
        near.stderr
    );
    assert!(!near.stderr.contains("@acme/postcodes"), "{}", near.stderr);
    assert!(!world.app.join("jadpo.lock").exists());

    // A bare name is Jadpo's own or nothing.
    let bare = with(&world.url, &world.app, &["package", "add", "postcodes"]);
    assert_eq!(bare.code, 1, "{}", bare.stderr);
    assert!(
        bare.stderr
            .starts_with("error: Jadpo has no official package `postcodes`\n"),
        "{}",
        bare.stderr
    );
    assert!(
        bare.stderr.contains("jadpo package search postcodes"),
        "{}",
        bare.stderr
    );

    // A release is `--to`, so nothing reads as a git tag.
    let tagged = with(
        &world.url,
        &world.app,
        &["package", "add", "@acme/postcodes@2.0.0"],
    );
    assert_eq!(tagged.code, 2, "{}", tagged.stderr);
    assert!(tagged.stderr.contains("--to 2.0.0"), "{}", tagged.stderr);
    let wrong = with(
        &world.url,
        &world.app,
        &["package", "add", "@Acme/Postcodes"],
    );
    assert_eq!(wrong.code, 2, "{}", wrong.stderr);
    let version = with(
        &world.url,
        &world.app,
        &["package", "add", "@acme/postcodes", "--to", "9.0.0"],
    );
    assert_eq!(version.code, 1, "{}", version.stderr);
    assert!(
        version.stderr.contains("Its releases are `2.0.0`."),
        "{}",
        version.stderr
    );
}

#[test]
fn a_yanked_release_is_refused_and_the_newest_good_one_taken() {
    let world = world("registry-yank", &days_ago(40));
    second_release(&world, &days_ago(10));
    world
        .registry
        .yank(&name("@acme/postcodes"), &version("2.1.0"));
    let asked = with(
        &world.url,
        &world.app,
        &["package", "add", "@acme/postcodes", "--to", "2.1.0"],
    );
    assert_eq!(asked.code, 1, "{}", asked.stderr);
    assert!(
        asked.stderr.contains("rule: install.yanked\n"),
        "{}",
        asked.stderr
    );
    assert!(!world.app.join("jadpo.lock").exists());
    let added = with(
        &world.url,
        &world.app,
        &["package", "add", "@acme/postcodes"],
    );
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    assert!(
        added.stdout.starts_with("Added Postcodes.v2 2.0.0\n"),
        "{}",
        added.stdout
    );
}

#[test]
fn nothing_is_installed_that_the_log_doesnt_prove() {
    let world = world("registry-proof", &days_ago(40));

    // An archive with other files than the digest the log holds.
    let (_held, url, _) = stand_in(&world.registry, |incoming, answer| {
        if incoming.target.contains("/archives/") {
            let files = vec![(
                "package.jadpo".to_string(),
                b"package Other.v1 {}\n".to_vec(),
            )];
            return Fetched {
                body: jadpo_registry::archive::pack(&files).unwrap(),
                ..answer
            };
        }
        answer
    });
    let swapped = with(&url, &world.app, &["package", "add", "@acme/postcodes"]);
    assert_eq!(swapped.code, 1, "{}", swapped.stderr);
    assert!(
        swapped.stderr.contains("rule: install.not_proved\n"),
        "{}",
        swapped.stderr
    );
    assert!(!world.app.join("jadpo.lock").exists());
    assert!(!world.app.join("packages").exists());

    // A proof that isn't of this release.
    let (_held, url, _) = stand_in(&world.registry, |incoming, answer| {
        if incoming.target.contains("/log/proof/") {
            let mut json: serde_json::Value = serde_json::from_slice(&answer.body).unwrap();
            json["hashes"] = serde_json::json!(["00".repeat(32)]);
            return Fetched {
                body: serde_json::to_vec(&json).unwrap(),
                ..answer
            };
        }
        answer
    });
    let unproved = with(&url, &world.app, &["package", "add", "@acme/postcodes"]);
    assert_eq!(unproved.code, 1, "{}", unproved.stderr);
    assert!(
        unproved.stderr.contains("rule: install.not_proved\n"),
        "{}",
        unproved.stderr
    );

    // A release it never logged.
    let (_held, url, _) = stand_in(&world.registry, |incoming, answer| {
        if incoming.target.contains("/log/proof/") {
            return Fetched {
                status: 404,
                content_type: Some("application/json".into()),
                body: br#"{"error":{"code":"not_in_log","message":"No.","request_id":"r"}}"#
                    .to_vec(),
            };
        }
        answer
    });
    let unlogged = with(&url, &world.app, &["package", "add", "@acme/postcodes"]);
    assert_eq!(unlogged.code, 1, "{}", unlogged.stderr);
    assert!(
        unlogged.stderr.contains("rule: install.not_proved\n"),
        "{}",
        unlogged.stderr
    );

    // Another log than the one seen before: the same app, a second package.
    let added = with(
        &world.url,
        &world.app,
        &["package", "add", "@acme/postcodes"],
    );
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    let head = std::fs::read_to_string(world.app.join(".jadpo/registry_head")).unwrap();
    let other = LocalRegistry::new();
    let mut different = ReleaseSeed::new(
        name("@evil/other"),
        version("1.0.0"),
        snapshot(&world.package),
    );
    different.released_at = days_ago(9);
    other.release(different);
    let mut more = ReleaseSeed::new(
        name("@evil/more"),
        version("1.0.0"),
        snapshot(&world.package),
    );
    more.released_at = days_ago(9);
    other.release(more);
    let forked = other.serve().expect("serves");
    let refused = with(&forked.url(), &world.app, &["package", "add", "@evil/more"]);
    assert_eq!(refused.code, 1, "{}", refused.stderr);
    assert!(
        refused.stderr.contains("rule: install.not_proved\n"),
        "{}",
        refused.stderr
    );
    // The head kept isn't moved by what was refused.
    assert_eq!(
        std::fs::read_to_string(world.app.join(".jadpo/registry_head")).unwrap(),
        head
    );
    drop(world.served);
}

#[test]
fn an_archive_is_read_as_raw_bytes_up_to_a_releases_limit() {
    let (dir, package, app) = workspace("registry-large");
    write(&package, "GUIDE.md", &"A long guide.\n".repeat(490_000));
    let files = snapshot(&package);
    let packed = jadpo_registry::archive::pack(&files).unwrap();
    assert!(
        (6_000_000..=7_000_000).contains(&packed.len()),
        "{}",
        packed.len()
    );
    let registry = LocalRegistry::new();
    registry.publisher("@acme", true, false);
    seed(&registry, &package, "2.0.0", &days_ago(9), Vec::new());
    // However it is labelled, and however the registry sends it.
    let (_held, url, _) = stand_in(&registry, |incoming, answer| {
        if incoming.target.contains("/archives/") {
            return Fetched {
                content_type: Some("application/octet-stream".into()),
                ..answer
            };
        }
        answer
    });
    let added = with(&url, &app, &["package", "add", "@acme/postcodes"]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    assert_eq!(
        std::fs::read(app.join("packages/postcodes/v2/GUIDE.md"))
            .unwrap()
            .len(),
        "A long guide.\n".len() * 490_000
    );
    drop(dir);
}

#[test]
fn a_registry_out_of_reach_is_said_as_that() {
    let (dir, _package, app) = workspace("registry-out-of-reach");
    let closed = with(
        "http://127.0.0.1:9/registry/v1/",
        &app,
        &["package", "add", "@acme/postcodes"],
    );
    assert_eq!(closed.code, 1, "{}", closed.stderr);
    assert!(
        closed.stderr.contains("rule: registry.unreachable\n"),
        "{}",
        closed.stderr
    );
    // Plain HTTP to another machine is never tried.
    let refused = with(
        "http://registry.example/registry/v1/",
        &app,
        &["package", "add", "@acme/postcodes"],
    );
    assert_eq!(refused.code, 1, "{}", refused.stderr);
    assert!(
        refused.stderr.contains("rule: registry.unreachable\n"),
        "{}",
        refused.stderr
    );
    drop(dir);
}

/// A 404 or a 405 with no failure in it isn't a network that failed, nor a
/// package the registry lacks: nothing there speaks the protocol, and every
/// command says so rather than sending the reader to check their connection.
#[test]
fn a_404_that_isnt_the_protocols_says_no_registry_is_there() {
    let (dir, _package, app) = workspace("registry-nothing-served");
    for status in [404, 405] {
        let server = Server::start(Arc::new(move |_: Incoming| Fetched {
            status,
            content_type: Some("text/html".to_string()),
            body: b"<h1>Not here</h1>".to_vec(),
        }))
        .expect("serves");
        let url = format!("http://{}/registry/v1/", server.address());
        for args in [
            &["package", "add", "@acme/postcodes"][..],
            &["package", "add", "postcodes"][..],
            &["package", "search", "postcode"][..],
        ] {
            let out = with(&url, &app, args);
            assert_eq!(out.code, 1, "{args:?}: {}", out.stderr);
            for words in [
                "isn't serving a registry".to_string(),
                format!("It answered {status} "),
                "next: Check `JADPO_REGISTRY`".to_string(),
                "rule: registry.unreachable\n".to_string(),
            ] {
                assert!(out.stderr.contains(&words), "{words}\n{}", out.stderr);
            }
            assert!(
                !out.stderr.contains("Check your connection"),
                "{}",
                out.stderr
            );
        }
    }
    drop(dir);
}

// --- update ----------------------------------------------------------------

/// An app with 2.0.0 installed from the registry.
fn installed(tag: &str) -> World {
    let world = world(tag, &days_ago(40));
    let added = with(
        &world.url,
        &world.app,
        &["package", "add", "@acme/postcodes"],
    );
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    world
}

#[test]
fn update_waits_three_days_and_names_what_it_skipped() {
    let world = installed("registry-wait");
    second_release(&world, &days_ago(1));
    let waited = with(&world.url, &world.app, &["package", "update", "postcodes"]);
    assert_eq!(waited.code, 0, "{}{}", waited.stdout, waited.stderr);
    assert!(
        waited.stdout.contains("Postcodes.v2 is up to date"),
        "{}",
        waited.stdout
    );
    assert!(waited.stdout.contains("2.1.0"), "{}", waited.stdout);
    assert!(waited.stdout.contains("1 day old"), "{}", waited.stdout);
    assert!(
        std::fs::read_to_string(world.app.join("jadpo.lock"))
            .unwrap()
            .contains("version 2.0.0"),
    );
    let (document, _) = json(&world.url, &world.app, &["package", "update", "postcodes"]);
    assert_eq!(document["data"]["updated"], false);
    assert_eq!(document["data"]["waiting"][0]["version"], "2.1.0");

    // `--to` takes it at once, after showing its facts.
    let taken = with(
        &world.url,
        &world.app,
        &["package", "update", "postcodes", "--to", "2.1.0"],
    );
    assert_eq!(taken.code, 0, "{}{}", taken.stdout, taken.stderr);
    assert!(
        taken
            .stdout
            .starts_with("Updated Postcodes.v2 from 2.0.0 to 2.1.0\n"),
        "{}",
        taken.stdout
    );
    assert!(taken.stdout.contains("kind:"), "{}", taken.stdout);
    match &lock(&world.app)[0].source {
        Source::Registry { version, .. } => assert_eq!(version, "2.1.0"),
        other => panic!("{other:?}"),
    }
    // The log grew, and the head moved with it.
    let head = std::fs::read_to_string(world.app.join(".jadpo/registry_head")).unwrap();
    assert!(head.starts_with("2 "), "{head}");
}

#[test]
fn update_takes_the_newest_release_three_days_old_with_its_change_record() {
    let world = installed("registry-old-enough");
    second_release(&world, &days_ago(4));
    // A package from the registry is named by its registry name too.
    let updated = with(
        &world.url,
        &world.app,
        &["package", "update", "@acme/postcodes"],
    );
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    assert!(
        updated
            .stdout
            .starts_with("Updated Postcodes.v2 from 2.0.0 to 2.1.0\n"),
        "{}",
        updated.stdout
    );
    assert!(
        updated
            .stdout
            .contains("source    registry @acme/postcodes\n"),
        "{}",
        updated.stdout
    );
    assert!(updated.stdout.contains("released  "), "{}", updated.stdout);
    assert!(
        updated.stdout.contains("Change record, 2.0.0 to 2.1.0"),
        "{}",
        updated.stdout
    );
    assert!(
        updated.stdout.contains("type OutwardCode"),
        "{}",
        updated.stdout
    );
    let again = with(&world.url, &world.app, &["package", "update", "postcodes"]);
    assert_eq!(again.code, 0, "{}", again.stdout);
    assert!(again.stdout.contains("up to date"), "{}", again.stdout);
    // `--to` is the registry's.
    assert_eq!(plain(&world.app, &["check"]).code, 0);
}

#[test]
fn an_urgent_release_skips_the_wait_and_says_why() {
    let world = installed("registry-urgent");
    second_release(&world, &days_ago(0));
    world
        .registry
        .urgent(&name("@acme/postcodes"), &version("2.1.0"));
    world.registry.advisory(Advisory {
        id: "ADV-7".into(),
        package: name("@acme/postcodes"),
        declarations: vec!["Postcodes.uk.parse".into()],
        from: version("2.0.0"),
        to: version("2.0.0"),
        summary: "Reads the wrong outward code.".into(),
        copies: Vec::new(),
    });
    let updated = with(&world.url, &world.app, &["package", "update", "postcodes"]);
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    assert!(
        updated
            .stdout
            .starts_with("Updated Postcodes.v2 from 2.0.0 to 2.1.0\n"),
        "{}",
        updated.stdout
    );
    assert!(
        updated
            .stdout
            .contains("urgent    ADV-7: Reads the wrong outward code.\n"),
        "{}",
        updated.stdout
    );
}

#[test]
fn a_new_major_is_named_and_never_taken_by_update() {
    let world = installed("registry-major");
    let mut major = ReleaseSeed::new(
        name("@acme/postcodes"),
        version("3.0.0"),
        snapshot(&world.package),
    );
    major.released_at = days_ago(30);
    world.registry.release(major);
    let updated = with(&world.url, &world.app, &["package", "update", "postcodes"]);
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    assert!(updated.stdout.contains("up to date"), "{}", updated.stdout);
    assert!(updated.stdout.contains("3.0.0"), "{}", updated.stdout);
    assert!(updated.stdout.contains("a new major"), "{}", updated.stdout);
}

#[test]
fn a_package_handed_to_another_publisher_is_shown_and_waits_again() {
    let world = installed("registry-moved");
    second_release(&world, &days_ago(1));
    world.registry.publisher("@new", false, false);
    // A copy of the package at 2.2.0: its surface says so.
    let mut files = snapshot(&world.package);
    let mut surface = String::new();
    for (path, bytes) in &mut files {
        if path == "surface.jadpo_record" {
            surface = String::from_utf8_lossy(bytes)
                .replace("version 2.1.0", "version 2.2.0")
                .replace("publisher @acme", "publisher @new");
            *bytes = surface.clone().into_bytes();
        }
        if path == "package.jadpo" {
            *bytes = String::from_utf8_lossy(bytes)
                .replace("@acme", "@new")
                .into_bytes();
        }
    }
    let mut carried = ReleaseSeed::new(name("@new/postcodes"), version("2.2.0"), files);
    carried.released_at = days_ago(1);
    carried.surface = surface;
    world.registry.release(carried);
    world
        .registry
        .urgent(&name("@new/postcodes"), &version("2.2.0"));
    world
        .registry
        .unmaintained(&name("@acme/postcodes"), Some(name("@new/postcodes")));

    // Under a new publisher the wait starts again, and urgent doesn't skip it.
    let waited = with(&world.url, &world.app, &["package", "update", "postcodes"]);
    assert_eq!(waited.code, 0, "{}{}", waited.stdout, waited.stderr);
    assert!(waited.stdout.contains("up to date"), "{}", waited.stdout);
    assert!(
        waited.stdout.contains("@new/postcodes"),
        "{}",
        waited.stdout
    );

    // `--to` takes it, and the review shows the new publisher.
    let taken = with(
        &world.url,
        &world.app,
        &["package", "update", "postcodes", "--to", "2.2.0"],
    );
    assert_eq!(taken.code, 0, "{}{}", taken.stdout, taken.stderr);
    assert!(
        taken.stdout.contains("New publisher since 2.0.0"),
        "{}",
        taken.stdout
    );
    assert!(taken.stdout.contains("@new, was @acme"), "{}", taken.stdout);
    match &lock(&world.app)[0].source {
        Source::Registry { name, version, .. } => {
            assert_eq!(name, "@new/postcodes");
            assert_eq!(version, "2.2.0");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_puts_back_a_copy_changed_by_hand_from_the_registry() {
    let world = installed("registry-again");
    write(&world.app, "packages/postcodes/v2/GUIDE.md", "changed\n");
    let again = with(&world.url, &world.app, &["package", "update", "postcodes"]);
    assert_eq!(again.code, 0, "{}{}", again.stdout, again.stderr);
    assert!(
        again
            .stdout
            .starts_with("Installed Postcodes.v2 2.0.0 again from its source\n"),
        "{}",
        again.stdout
    );
    let guide = std::fs::read_to_string(world.app.join("packages/postcodes/v2/GUIDE.md")).unwrap();
    assert_ne!(guide, "changed\n");
}

#[test]
fn to_goes_with_a_package_from_the_registry() {
    let (_dir, package, app) = workspace("registry-to-git");
    let added = plain(&app, &["package", "add", &package.to_string_lossy()]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    let refused = plain(&app, &["package", "update", "postcodes", "--to", "2.1.0"]);
    assert_eq!(refused.code, 2, "{}", refused.stderr);
    assert!(
        refused.stderr.contains("came from a folder or git"),
        "{}",
        refused.stderr
    );
}

// --- search ----------------------------------------------------------------

#[test]
fn search_finds_the_standard_library_first_and_sends_shapes_never_source() {
    let world = installed("registry-search");
    world
        .registry
        .standard("text.upper", "Upper-cases a text, such as a postcode.");
    world.registry.account(
        &name("@acme/postcodes"),
        "uk.parse",
        "Reads a UK postcode as people type it.",
    );
    let (_held, url, asked) = stand_in(&world.registry, |_, answer| answer);
    let found = with(
        &url,
        &world.app,
        &["package", "search", "uk", "postcode", "reader"],
    );
    assert_eq!(found.code, 0, "{}{}", found.stdout, found.stderr);
    let standard = found
        .stdout
        .find("text.upper")
        .expect("the standard library");
    let package = found.stdout.find("uk.parse").expect("the package");
    assert!(standard < package, "{}", found.stdout);
    assert!(found.stdout.contains("@acme/postcodes"), "{}", found.stdout);
    assert_eq!(
        asked.lock().unwrap().last().unwrap(),
        "/registry/v1/search?q=uk%20postcode%20reader"
    );

    // `--to` a type of the app's goes as its shape, and `--kind` as it is.
    write(
        &world.app,
        "domains/main/types/ticket.jadpo",
        "/// A support ticket's reference.\npublic type TicketReference = Text {\n    max_length: 12\n}\n",
    );
    let shaped = with(
        &url,
        &world.app,
        &[
            "package",
            "search",
            "tickets",
            "--to",
            "TicketReference",
            "--from",
            "Text",
            "--kind",
            "library",
        ],
    );
    assert_eq!(shaped.code, 0, "{}{}", shaped.stdout, shaped.stderr);
    let target = asked.lock().unwrap().last().unwrap().clone();
    assert!(target.contains("from=Text"), "{target}");
    assert!(target.contains("kind=library"), "{target}");
    assert!(
        target.contains("to=Text%20%7B%20max_length%3A%2012%20%7D"),
        "{target}"
    );
    assert!(!target.contains("TicketReference"), "{target}");

    let unknown = with(
        &url,
        &world.app,
        &["package", "search", "tickets", "--to", "TicketReferense"],
    );
    assert_eq!(unknown.code, 2, "{}", unknown.stderr);
    assert!(
        unknown.stderr.contains("TicketReference"),
        "{}",
        unknown.stderr
    );

    let nothing = with(&url, &world.app, &["package", "search", "zzzz"]);
    assert_eq!(nothing.code, 0, "{}{}", nothing.stdout, nothing.stderr);
    assert!(
        nothing.stdout.contains("nothing found"),
        "{}",
        nothing.stdout
    );

    let none = with(&url, &world.app, &["package", "search"]);
    assert_eq!(none.code, 2, "{}", none.stderr);
}

// --- audit -----------------------------------------------------------------

fn advisory(id: &str, declaration: &str, to: &str) -> Advisory {
    Advisory {
        id: id.into(),
        package: name("@acme/postcodes"),
        declarations: vec![declaration.into()],
        from: version("2.0.0"),
        to: version(to),
        summary: format!("{id} summary."),
        copies: vec![name("@other/copied")],
    }
}

#[test]
fn audit_says_whether_the_app_reaches_each_advisory() {
    let world = installed("registry-audit");
    write(
        &world.app,
        "domains/main/actions/check_postcode.jadpo",
        USES,
    );
    let clean = with(&world.url, &world.app, &["package", "audit"]);
    assert_eq!(clean.code, 0, "{}{}", clean.stdout, clean.stderr);
    assert!(clean.stdout.contains("no advisories"), "{}", clean.stdout);

    // Not reached: said, and CI goes on.
    world
        .registry
        .advisory(advisory("ADV-1", "Postcodes.OutwardCode", "2.0.0"));
    let quiet = with(&world.url, &world.app, &["package", "audit"]);
    assert_eq!(quiet.code, 0, "{}{}", quiet.stdout, quiet.stderr);
    assert!(quiet.stdout.contains("ADV-1"), "{}", quiet.stdout);
    assert!(quiet.stdout.contains("not reached"), "{}", quiet.stdout);

    // Reached: an error, in CI too.
    world
        .registry
        .advisory(advisory("ADV-2", "Postcodes.uk.parse", "2.0.0"));
    let reached = with(&world.url, &world.app, &["package", "audit"]);
    assert_eq!(reached.code, 1, "{}{}", reached.stdout, reached.stderr);
    assert!(
        reached.stdout.contains("rule: package.advisory\n"),
        "{}",
        reached.stdout
    );
    assert!(
        reached.stdout.contains("Postcodes.uk.parse"),
        "{}",
        reached.stdout
    );
    assert!(
        reached.stdout.contains("@other/copied"),
        "{}",
        reached.stdout
    );

    let (document, code) = json(&world.url, &world.app, &["package", "audit"]);
    assert_eq!(code, 1);
    let advisories = document["data"]["advisories"].as_array().expect("a list");
    assert_eq!(advisories.len(), 2);
    assert_eq!(advisories[0]["id"], "ADV-1");
    assert_eq!(advisories[0]["reached"], false);
    assert_eq!(advisories[1]["reached"], true);
}

#[test]
fn audit_only_asks_about_packages_from_the_registry() {
    let (_dir, package, app) = workspace("registry-audit-git");
    let added = plain(&app, &["package", "add", &package.to_string_lossy()]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    // Nothing to ask, so nothing is sent: this address is closed.
    let audited = with(
        "http://127.0.0.1:9/registry/v1/",
        &app,
        &["package", "audit"],
    );
    assert_eq!(audited.code, 0, "{}{}", audited.stdout, audited.stderr);
    assert!(
        audited.stdout.contains("no packages from the registry"),
        "{}",
        audited.stdout
    );
}

// --- publish ---------------------------------------------------------------

/// A package ready to publish at 2.0.0, its `origin` an HTTPS address git
/// reads from a bare repository on this machine.
fn ready(tag: &str) -> World {
    let world = world(tag, &days_ago(40));
    let log = std::fs::read_to_string(world.package.join("CHANGELOG.md")).expect("changelog");
    write(
        &world.package,
        "CHANGELOG.md",
        &format!("{log}\nReads UK postcodes as people type them.\n"),
    );
    let checked = plain(&world.package, &["package", "check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    release(&world.package, "v2.0.0");
    let bare = world.package.with_file_name("origin.git");
    let status = std::process::Command::new("git")
        .args(["init", "-q", "--bare"])
        .arg(&bare)
        .status()
        .expect("git");
    assert!(status.success());
    git(
        &world.package,
        &[
            "remote",
            "add",
            "origin",
            "https://git.example/acme/postcodes.git",
        ],
    );
    git(
        &world.package,
        &[
            "config",
            &format!("url.{}.insteadOf", bare.display()),
            "https://git.example/acme/postcodes.git",
        ],
    );
    world
}

fn push_tag(package: &Path) {
    git(package, &["push", "-q", "origin", "main", "v2.0.0"]);
}

/// Settles the first release started as soon as it has started.
fn settle_when_started(
    registry: &LocalRegistry,
    state: ReleaseState,
    bar: Vec<(String, bool, String)>,
) -> std::thread::JoinHandle<()> {
    let registry = registry.clone();
    std::thread::spawn(move || {
        for _ in 0..600 {
            if !registry.started().is_empty() {
                registry.settle("rel_0001", state, bar);
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    })
}

#[test]
fn publish_asks_for_a_repository_at_a_tag_and_a_person_confirms_it() {
    let world = ready("registry-publish");
    push_tag(&world.package);
    let settling = settle_when_started(
        &world.registry,
        ReleaseState::Pending,
        vec![("tests".to_string(), true, "3 tests".to_string())],
    );
    let published = with(&world.url, &world.package, &["package", "publish"]);
    settling.join().unwrap();
    assert_eq!(
        published.code, 0,
        "{}{}",
        published.stdout, published.stderr
    );
    assert_eq!(
        world.registry.started(),
        vec![(
            "https://git.example/acme/postcodes".to_string(),
            "v2.0.0".to_string(),
            false
        )]
    );
    // It ends at the page where it is published, on the site whose
    // registry it asked.
    let page = format!(
        "{}account/release?id=rel_0001",
        world.url.strip_suffix("registry/v1/").expect("a registry")
    );
    for line in [
        "Postcodes.v2 2.0.0",
        "https://git.example/acme/postcodes",
        "v2.0.0",
        "checked: Postcodes.v2 2.0.0, not published yet\n",
        &format!("next: {page}  publish it there with your passkey\n"),
    ] {
        assert!(
            published.stdout.contains(line),
            "{line}: {}",
            published.stdout
        );
    }
}

#[test]
fn a_dry_run_runs_the_verifier_and_publishes_nothing() {
    let world = ready("registry-dry-run");
    push_tag(&world.package);
    let settling = settle_when_started(
        &world.registry,
        ReleaseState::Pending,
        vec![("tests".to_string(), true, "3 tests".to_string())],
    );
    let (document, code) = json(
        &world.url,
        &world.package,
        &["package", "publish", "--dry-run"],
    );
    settling.join().unwrap();
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["data"]["dry_run"], true);
    assert_eq!(document["data"]["state"], "pending");
    assert_eq!(document["data"]["bar"][0]["check"], "tests");
    assert!(world.registry.started()[0].2, "a dry run");
}

#[test]
fn a_release_the_verifier_refuses_shows_its_rows() {
    let world = ready("registry-refused");
    push_tag(&world.package);
    let settling = settle_when_started(
        &world.registry,
        ReleaseState::Failed,
        vec![
            ("compiles".to_string(), true, "no errors".to_string()),
            ("tests".to_string(), false, "1 of 3 fails".to_string()),
        ],
    );
    let refused = with(
        &world.url,
        &world.package,
        &["package", "publish", "--dry-run"],
    );
    settling.join().unwrap();
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(
        refused.stdout.contains("rule: publish.failed\n"),
        "{}",
        refused.stdout
    );
    assert!(
        refused.stdout.contains("tests: 1 of 3 fails"),
        "{}",
        refused.stdout
    );
}

/// The verifier's answer, once started, put through the registry's own check
/// of whose release it is.
fn check_when_started(registry: &LocalRegistry, publisher: &str) -> std::thread::JoinHandle<()> {
    let registry = registry.clone();
    let publisher = Handle::parse(publisher).expect("a handle");
    std::thread::spawn(move || {
        for _ in 0..600 {
            if !registry.started().is_empty() {
                let mut facts =
                    ReleaseSeed::new(name("@acme/postcodes"), version("2.0.0"), Vec::new()).facts;
                facts.publisher = Some(publisher);
                let bar = vec![("tests".to_string(), true, "3 tests".to_string())];
                registry.checked("rel_0001", "Postcodes", facts, bar);
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    })
}

#[test]
fn publish_goes_through_the_registrys_check_of_its_publisher() {
    let world = ready("registry-publisher-passes");
    push_tag(&world.package);
    world.registry.repository_of(
        &name("@acme/postcodes"),
        "https://git.example/acme/postcodes",
    );
    let checking = check_when_started(&world.registry, "@acme");
    let published = with(&world.url, &world.package, &["package", "publish"]);
    checking.join().unwrap();
    assert_eq!(
        published.code, 0,
        "{}{}",
        published.stdout, published.stderr
    );
    assert!(
        published.stdout.contains("publisher   @acme\n"),
        "{}",
        published.stdout
    );
    assert!(
        published.stdout.contains("checked: Postcodes.v2 2.0.0"),
        "{}",
        published.stdout
    );
    // It said who the release is for when it asked.
    assert_eq!(
        world.registry.started_for(),
        vec![Some(Handle::parse("@acme").expect("a handle"))]
    );
}

/// A repository another publisher releases from is refused when publishing
/// starts, before any check, in the registry's own words.
#[test]
fn publish_shows_the_registrys_own_reason_for_refusing_a_repository() {
    let world = ready("registry-publisher-elsewhere");
    push_tag(&world.package);
    world.registry.publisher("@other", false, false);
    let mut seeded = ReleaseSeed::new(
        name("@other/postcodes"),
        version("1.0.0"),
        snapshot(&world.package),
    );
    seeded.surface =
        std::fs::read_to_string(world.package.join("surface.jadpo_record")).expect("surface");
    world.registry.release(seeded);
    world.registry.repository_of(
        &name("@other/postcodes"),
        "https://git.example/acme/postcodes",
    );
    let refused = with(&world.url, &world.package, &["package", "publish"]);
    let shown = format!("{}{}", refused.stdout, refused.stderr);
    assert_eq!(refused.code, 1, "{shown}");
    assert!(
        shown.contains("rule: publish.repository_refused\n"),
        "{shown}"
    );
    assert!(
        shown.contains(
            "reason: `https://git.example/acme/postcodes` releases `@other/postcodes`, which is `@other`'s, not `@acme`'s.\n"
        ),
        "{shown}"
    );
    assert!(!shown.contains("reached over HTTPS"), "{shown}");
    assert!(world.registry.started().is_empty());
}

#[test]
fn a_fork_naming_the_original_is_refused_with_its_own_page() {
    let world = ready("registry-publisher-fork");
    push_tag(&world.package);
    // `@acme/postcodes` comes from another repository than this checkout's.
    let checking = check_when_started(&world.registry, "@acme");
    let refused = with(&world.url, &world.package, &["package", "publish"]);
    checking.join().unwrap();
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(
        refused
            .stdout
            .contains("rule: publish.publisher_mismatch\n"),
        "{}",
        refused.stdout
    );
    assert!(
        refused.stdout.contains("https://github.com/acme/postcodes"),
        "{}",
        refused.stdout
    );
    assert!(
        !refused.stdout.contains("rule: publish.failed"),
        "{}",
        refused.stdout
    );
}

#[test]
fn a_release_the_registry_read_as_another_publishers_is_refused() {
    let world = ready("registry-publisher-disagrees");
    push_tag(&world.package);
    // The registry holds nothing of `@other`'s, so it has nothing to refuse;
    // this checkout says `@acme`, and that is who asked.
    let checking = check_when_started(&world.registry, "@other");
    let refused = with(&world.url, &world.package, &["package", "publish"]);
    checking.join().unwrap();
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(
        refused
            .stdout
            .contains("rule: publish.publisher_mismatch\n")
            || refused
                .stderr
                .contains("rule: publish.publisher_mismatch\n"),
        "{}{}",
        refused.stdout,
        refused.stderr
    );
    let said = format!("{}{}", refused.stdout, refused.stderr);
    assert!(said.contains("`@other`"), "{said}");
    assert!(said.contains("`@acme`"), "{said}");
}

#[test]
fn publish_refuses_a_package_with_no_publisher_and_asks_nothing() {
    let world = ready("registry-publisher-missing");
    write(
        &world.package,
        "package.jadpo",
        &super::packages::INDEX.replace("    publisher: \"@acme\"\n", ""),
    );
    plain(&world.package, &["package", "check"]);
    let log = std::fs::read_to_string(world.package.join("CHANGELOG.md")).expect("changelog");
    write(
        &world.package,
        "CHANGELOG.md",
        &log.replacen("## 2.0.0", "No longer names a publisher.\n\n## 2.0.0", 1),
    );
    let checked = plain(&world.package, &["package", "check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    git(&world.package, &["add", "-A"]);
    git(&world.package, &["commit", "-q", "-m", "no publisher"]);
    git(&world.package, &["tag", "v2.0.1"]);
    git(&world.package, &["push", "-q", "origin", "main", "v2.0.1"]);
    let refused = with(&world.url, &world.package, &["package", "publish"]);
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(
        refused.stdout.contains("rule: publish.publisher_missing\n"),
        "{}",
        refused.stdout
    );
    assert!(world.registry.started().is_empty());
}

#[test]
fn publish_refuses_a_proprietary_package_and_asks_nothing() {
    let world = ready("registry-proprietary");
    // Its licence is on the surface, so going private is a new major version.
    write(
        &world.package,
        "package.jadpo",
        &super::packages::INDEX
            .replace("Licence.mit", "Licence.proprietary")
            .replace("Postcodes.v2", "Postcodes.v3"),
    );
    plain(&world.package, &["package", "check"]);
    let log = std::fs::read_to_string(world.package.join("CHANGELOG.md")).expect("changelog");
    write(
        &world.package,
        "CHANGELOG.md",
        &log.replacen("## 2.0.0", "Now private to Acme.\n\n## 2.0.0", 1),
    );
    let checked = plain(&world.package, &["package", "check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    git(&world.package, &["add", "-A"]);
    git(&world.package, &["commit", "-q", "-m", "private"]);
    git(&world.package, &["tag", "v3.0.0"]);
    git(&world.package, &["push", "-q", "origin", "main", "v3.0.0"]);
    let refused = with(&world.url, &world.package, &["package", "publish"]);
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(
        refused
            .stdout
            .contains("rule: publish.licence_proprietary\n"),
        "{}",
        refused.stdout
    );
    assert!(world.registry.started().is_empty());
}

#[test]
fn add_refuses_files_that_name_another_publisher() {
    let (_dir, package, app) = workspace("registry-add-publisher");
    let registry = LocalRegistry::new();
    // `@other/postcodes` whose files say they are `@acme`'s.
    let mut seed = ReleaseSeed::new(
        name("@other/postcodes"),
        version("2.0.0"),
        snapshot(&package),
    );
    seed.released_at = days_ago(40);
    seed.surface = std::fs::read_to_string(package.join("surface.jadpo_record")).expect("surface");
    registry.release(seed);
    let served = registry.serve().expect("serves");
    let refused = with(&served.url(), &app, &["package", "add", "@other/postcodes"]);
    assert_eq!(refused.code, 1, "{}", refused.stderr);
    assert!(
        refused.stderr.contains("rule: install.not_proved\n"),
        "{}",
        refused.stderr
    );
    assert!(
        refused
            .stderr
            .contains("Its name says `@other`, and its files say `@acme`."),
        "{}",
        refused.stderr
    );
    assert!(!app.join("jadpo.lock").exists());
}

#[test]
fn publish_says_what_isnt_ready_and_asks_nothing() {
    let world = ready("registry-not-ready");
    // The tag isn't pushed.
    let unpushed = with(&world.url, &world.package, &["package", "publish"]);
    assert_eq!(unpushed.code, 1, "{}", unpushed.stderr);
    assert!(
        unpushed.stderr.contains("rule: publish.not_at_tag\n"),
        "{}",
        unpushed.stderr
    );
    assert!(
        unpushed.stderr.contains("git push origin v2.0.0"),
        "{}",
        unpushed.stderr
    );

    // A change after the tag isn't what the tag holds.
    push_tag(&world.package);
    write(&world.package, "GUIDE.md", "# Postcodes\n\nChanged.\n");
    let changed = with(&world.url, &world.package, &["package", "publish"]);
    assert_eq!(changed.code, 1, "{}", changed.stderr);
    assert!(
        changed.stderr.contains("rule: publish.not_at_tag\n"),
        "{}",
        changed.stderr
    );
    git(&world.package, &["checkout", "-q", "GUIDE.md"]);

    // Only HTTPS.
    git(
        &world.package,
        &[
            "remote",
            "set-url",
            "origin",
            "http://git.example/acme/postcodes.git",
        ],
    );
    let http = with(&world.url, &world.package, &["package", "publish"]);
    assert_eq!(http.code, 1, "{}", http.stderr);
    assert!(
        http.stderr.contains("rule: publish.repository_refused\n"),
        "{}",
        http.stderr
    );
    assert!(world.registry.started().is_empty());

    // Not a package at all.
    let elsewhere = with(&world.url, &world.app, &["package", "publish"]);
    assert_eq!(elsewhere.code, 2, "{}", elsewhere.stderr);
}

// --- preview ---------------------------------------------------------------

#[test]
fn preview_serves_the_packages_page_with_nothing_published() {
    let world = ready("registry-preview");
    let session = plain(&world.package, &["package", "preview"]);
    assert_eq!(session.code, 0, "{}", session.stderr);
    let Some(jadpo_cli::Session::Preview(args)) = session.session else {
        panic!("a session: {}", session.stdout);
    };
    let served = jadpo_cli::package_preview::start(&args).expect("serves");
    let url = served.url();
    assert!(url.starts_with("http://127.0.0.1:"), "{url}");
    let page = fetch(Method::Get, &url, None).expect("answers");
    assert_eq!(page.status, 200);
    assert_eq!(page.content_type.as_deref(), Some("text/html"));
    let html = String::from_utf8(page.body).unwrap();
    for piece in [
        "Postcodes.v2",
        "2.0.0",
        "Nothing is published",
        "Facts",
        "Publishing bar",
        "Public surface",
        "Changelog",
        "UkPostcode",
        "jadpo package publish --dry-run",
    ] {
        assert!(html.contains(piece), "{piece}: {html}");
    }
    let missing = fetch(Method::Get, &format!("{url}nowhere"), None).expect("answers");
    assert_eq!(missing.status, 404);
    let post = fetch(Method::Post, &url, Some(b"{}")).expect("answers");
    assert_eq!(post.status, 405);
    let elsewhere = plain(&world.app, &["package", "preview"]);
    assert_eq!(elsewhere.code, 2, "{}", elsewhere.stderr);
}
