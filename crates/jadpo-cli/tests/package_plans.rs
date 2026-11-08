//! The blast radius of an update (area 14, "`jadpo package update` and
//! `remove`"; chunk R9): `jadpo package update <name> --plan` compiles the
//! app against the release in a scratch copy, follows each change record
//! entry through the app's graph to the actions, routes, jobs and stored
//! fields it reaches, and shows the compile errors and the migration that
//! would follow, writing nothing. Every package is a local folder: nothing
//! is fetched from the network.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_cli::run;
use jadpo_term::Environment;

use super::http::{jadpo, tempdir, write};

const INDEX: &str = "/// UK postcodes: reading and normalising them.
package Postcodes.v2 {
    publisher: \"@acme\"
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

const UK: &str = "/// Reading UK postcodes.
public library uk {
    /// Reads a UK postcode as it is written.
    function parse(written: WrittenPostcode) fails InvalidPostcode -> UkPostcode {
        return attempt UkPostcode(text.upper(written)) {
            invalid: InvalidPostcode
        }
    }

    /// A postcode tidied.
    function tidy(postcode: UkPostcode) -> UkPostcode {
        return postcode
    }
}
";

/// The app: an entity storing the package's type, an action a route runs
/// and one a schedule runs, each calling into the package.
const APP: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/places/entities/address.jadpo",
        "use packages/postcodes

/// Where a customer lives.
entity Address {
    id: Uuid
    postcode: Postcodes.UkPostcode

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
",
    ),
    (
        "domains/places/types/new_address.jadpo",
        "use packages/postcodes

/// An address to keep.
type NewAddress = Input {
    postcode: Postcodes.WrittenPostcode
}
",
    ),
    (
        "domains/places/types/kept_address.jadpo",
        "use packages/postcodes

/// An address kept.
type KeptAddress = Output {
    id: Uuid
    postcode: Postcodes.UkPostcode
}
",
    ),
    (
        "domains/places/actions/save_address.jadpo",
        "use packages/postcodes
use places/entities { Address }
use places/types { KeptAddress, NewAddress }

/// Keeps a customer's address.
action save_address(input: NewAddress) fails Postcodes.InvalidPostcode -> KeptAddress {
    var postcode: Postcodes.UkPostcode = attempt Postcodes.uk.parse(input.postcode)
    var address: Address = Address.create {
        set: {
            postcode: postcode
        }
    }
    return KeptAddress {
        id: address.id
        postcode: address.postcode
    }
}
",
    ),
    (
        "domains/places/actions/tidy_all.jadpo",
        "use packages/postcodes

/// Tidies the office's own postcode.
action tidy_all() fails Postcodes.InvalidPostcode -> Postcodes.UkPostcode {
    var office: Postcodes.UkPostcode = attempt Postcodes.uk.parse(\"sw1a 1aa\")
    return Postcodes.uk.tidy(office)
}
",
    ),
    (
        "domains/places/routes/addresses.jadpo",
        "use places/actions { save_address }
use places/types { KeptAddress, NewAddress }

/// Addresses.
routes Addresses {
    prefix: /addresses

    /// Keeps an address.
    POST / {
        authentication: RouteAuthentication.none
        input: NewAddress
        output: KeptAddress
        run: save_address(input)
    }
}
",
    ),
    (
        "domains/places/schedules/nightly_tidy.jadpo",
        "use places/actions { tidy_all }

/// Tidies postcodes every night.
schedule nightly_tidy {
    every: 1d
    run: tidy_all()
}
",
    ),
];

/// Runs the publishing bar so the folder's committed surface is its
/// code's, naming `version`.
fn surface_at(package: &Path, version: &str) {
    jadpo(package, &["package", "check"]);
    let path = package.join("surface.jadpo_record");
    let text = std::fs::read_to_string(&path).expect("the surface");
    let at = text
        .lines()
        .find(|line| line.starts_with("    version "))
        .expect("a version line")
        .to_string();
    std::fs::write(
        &path,
        text.replacen(&at, &format!("    version {version}"), 1),
    )
    .expect("written");
}

/// A package's folder at 2.0.0 and an app with it installed, checked,
/// migrated and its records written: `<dir>/postcodes` and `<dir>/shop`.
fn installed(name: &str) -> (tempdir::Dir, PathBuf, PathBuf) {
    let dir = tempdir::Dir::new(name);
    let package = dir.path().join("postcodes");
    for (path, text) in [
        ("package.jadpo", INDEX),
        ("types/uk_postcode.jadpo", POSTCODE),
        ("types/written_postcode.jadpo", WRITTEN),
        ("failures/invalid_postcode.jadpo", INVALID),
        ("libraries/uk.jadpo", UK),
        ("GUIDE.md", "# Postcodes\n\nParse what people type.\n"),
        (
            "CHANGELOG.md",
            "# Changelog\n\n## 2.0.0\n\n- first release\n\nThe first.\n",
        ),
    ] {
        write(&package, path, text);
    }
    surface_at(&package, "2.0.0");
    let app = dir.path().join("shop");
    for (path, text) in APP {
        write(&app, path, text);
    }
    let added = jadpo(&app, &["package", "add", "../postcodes"]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    let migrated = jadpo(&app, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let records = jadpo(&app, &["records", "update"]);
    assert_eq!(records.code, 0, "{}{}", records.stdout, records.stderr);
    let checked = jadpo(&app, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    (dir, package, app)
}

/// 2.1.0: `UkPostcode` asks for one more character, `uk.tidy` is gone and
/// `uk.outward` is new.
fn second_release(package: &Path) {
    let replace = |path: &str, from: &str, to: &str| {
        let text = std::fs::read_to_string(package.join(path)).expect("read");
        assert_eq!(text.matches(from).count(), 1, "{from:?} in {path}");
        std::fs::write(package.join(path), text.replacen(from, to, 1)).expect("written");
    };
    replace("types/uk_postcode.jadpo", "min_length: 5", "min_length: 6");
    replace(
        "libraries/uk.jadpo",
        "    /// A postcode tidied.\n    function tidy(",
        "    /// A postcode as an envelope shows it.\n    function outward(",
    );
    surface_at(package, "2.1.0");
}

/// Every file under `folder`, by path, with its bytes.
fn tree(folder: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut waiting = vec![folder.to_path_buf()];
    while let Some(at) = waiting.pop() {
        for entry in std::fs::read_dir(&at).expect("a folder") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                waiting.push(path);
            } else {
                let shown = path
                    .strip_prefix(folder)
                    .expect("inside")
                    .to_string_lossy()
                    .into_owned();
                out.insert(shown, std::fs::read(&path).expect("read"));
            }
        }
    }
    out
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The screen as a terminal shows it, without its colours.
fn on_a_terminal(cwd: &Path, args: &[&str]) -> (String, u8) {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = Environment::empty()
        .with_terminals(true, true, true)
        .with_var("TERM", "xterm-256color")
        .with_var("NO_COLOR", "1")
        .with_cwd(cwd);
    let output = run(&args, &env);
    (strip_ansi(&output.stdout), output.code)
}

/// The page's example (area 11, "How packages are installed and linked").
const PAGE: &str = "✓ Planned Postcodes.v2 from 2.0.0 to 2.1.0, nothing changed
    folder   packages/postcodes/v2
    source   path ../postcodes
    surface  matches its code

Change record, 2.0.0 to 2.1.0
  + added    function uk.outward
    reaches  nothing in the app
  ~ retyped  type UkPostcode      min_length: 6, was min_length: 5
    actions  save_address, tidy_all
    routes   POST /addresses
    jobs     schedule nightly_tidy
    stored   Address.postcode
  - removed  function uk.tidy
    actions  tidy_all
    jobs     schedule nightly_tidy

Would not compile
  ✗ Library `Postcodes.uk` has no function `tidy`
    at  domains/places/actions/tidy_all.jadpo:6

Records out of date
  ~ api/main.jadpo_record

Migration that would follow
  ~ field `Address.postcode` changed  needs a decision: refit

Needs a person
  ~ UkPostcode  field retyped  UkPostcode  Text { max_length: 8, min_length: 5…
  - uk.tidy     removed

✓ upgrade planned  2 reach the app · 1 error · 1 schema change · 2 need a person

→ jadpo package update postcodes  takes 2.1.0
";

#[test]
fn a_plan_follows_each_change_record_entry_through_the_app_and_changes_nothing() {
    let (_dir, package, app) = installed("plan-page");
    second_release(&package);
    let before = tree(&app);
    let (screen, code) = on_a_terminal(&app, &["package", "update", "postcodes", "--plan"]);
    assert_eq!(code, 0, "{screen}");
    assert_eq!(screen, PAGE);
    // Nothing in the project moved: not the package's folder, not the lock.
    assert_eq!(tree(&app), before);

    let done = jadpo(
        &app,
        &["package", "update", "postcodes", "--plan", "--json"],
    );
    assert_eq!(done.code, 0, "{}", done.stderr);
    let document: serde_json::Value = serde_json::from_str(&done.stdout).expect("JSON");
    let data = &document["data"];
    assert_eq!(data["package"], "Postcodes.v2");
    assert_eq!(data["from"], "2.0.0");
    assert_eq!(data["version"], "2.1.0");
    assert_eq!(data["updated"], false);
    let retyped = &data["change_record"][1];
    assert_eq!(retyped["declaration"], "UkPostcode");
    assert_eq!(retyped["change"], "retyped");
    assert_eq!(
        retyped["reaches"],
        serde_json::json!({
            "actions": ["save_address", "tidy_all"],
            "routes": ["POST /addresses"],
            "jobs": ["schedule nightly_tidy"],
            "stored": ["Address.postcode"],
        })
    );
    assert_eq!(
        data["change_record"][0]["reaches"]["actions"],
        serde_json::json!([])
    );
    assert_eq!(data["errors"][0]["rule"], "semantic.unknown_callee");
    assert_eq!(
        data["errors"][0]["path"],
        "domains/places/actions/tidy_all.jadpo"
    );
    assert_eq!(data["errors"][0]["line"], 6);
    assert_eq!(
        data["records"],
        serde_json::json!(["api/main.jadpo_record"])
    );
    assert_eq!(data["migration"][0]["decision"], "refit");
    assert_eq!(data["review"].as_array().map(Vec::len), Some(2));
    assert_eq!(tree(&app), before);

    // The update itself then takes what the plan showed.
    let updated = jadpo(&app, &["package", "update", "postcodes"]);
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    assert!(
        updated
            .stdout
            .starts_with("Updated Postcodes.v2 from 2.0.0 to 2.1.0\n"),
        "{}",
        updated.stdout
    );
    let checked = jadpo(&app, &["check"]);
    assert!(
        checked
            .stdout
            .contains("Library `Postcodes.uk` has no function `tidy`"),
        "{}",
        checked.stdout
    );
}

#[test]
fn a_plan_with_nothing_reaching_the_app_says_so() {
    let (_dir, package, app) = installed("plan-nothing");
    // Only an addition: nothing the app has can reach it.
    let text = std::fs::read_to_string(package.join("libraries/uk.jadpo")).expect("read");
    std::fs::write(
        package.join("libraries/uk.jadpo"),
        text.replacen(
            "\n}\n",
            "\n\n    /// A postcode as someone typed it, kept as it was.\n    function outward(written: WrittenPostcode) -> WrittenPostcode {\n        return written\n    }\n}\n",
            1,
        ),
    )
    .expect("written");
    surface_at(&package, "2.1.0");
    let done = jadpo(&app, &["package", "update", "postcodes", "--plan"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert!(
        done.stdout.contains(
            "Change record, 2.0.0 to 2.1.0\n  + added  function uk.outward\n    reaches  nothing in the app\n"
        ),
        "{}",
        done.stdout
    );
    assert!(
        done.stdout
            .contains("upgrade planned: nothing reaches the app\n"),
        "{}",
        done.stdout
    );
    assert!(
        !done.stdout.contains("Would not compile"),
        "{}",
        done.stdout
    );
    assert!(!done.stdout.contains("Migration"), "{}", done.stdout);
}

#[test]
fn a_plan_with_nothing_newer_is_up_to_date() {
    let (_dir, _package, app) = installed("plan-up-to-date");
    let done = jadpo(&app, &["package", "update", "postcodes", "--plan"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert!(
        done.stdout.contains("Postcodes.v2 is up to date"),
        "{}",
        done.stdout
    );
}

#[test]
fn plan_goes_with_update_only() {
    let (_dir, _package, app) = installed("plan-arguments");
    let done = jadpo(&app, &["package", "add", "../postcodes", "--plan"]);
    assert_eq!(done.code, 2, "{}", done.stdout);
    assert!(
        done.stderr
            .starts_with("error: `--plan` goes with `jadpo package update`\n"),
        "{}",
        done.stderr
    );
    assert!(
        done.stderr.contains("rule: cli.package_arguments"),
        "{}",
        done.stderr
    );
}
