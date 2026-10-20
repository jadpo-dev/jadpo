//! `jadpo package recipe <package> <recipe> --into <domain>` (area 11,
//! "Recipes"; chunk 7.6): a package's recipe copied into one of the app's
//! domains, all or nothing, never over a file the app has.

use super::http::{jadpo, tempdir, write};

/// An app with the `shipping` domain and its own integration `Couriers`
/// shipping the recipe `track_parcels`.
fn app(name: &str) -> tempdir::Dir {
    let dir = tempdir::Dir::new(name);
    let root = dir.path();
    write(root, "app.jadpo", "application {}\n");
    write(
        root,
        "domains/shipping/types/parcel_code.jadpo",
        "/// A parcel's tracking code.\ntype ParcelCode = Text {\n    max_length: 20\n}\n",
    );
    write(
        root,
        "shared/providers/couriers/v1/package.jadpo",
        "/// Parcel tracking.\npackage Couriers.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    );
    write(
        root,
        "shared/providers/couriers/v1/recipes/track_parcels/actions/track_parcel.jadpo",
        "// Tracks one parcel.\n",
    );
    write(
        root,
        "shared/providers/couriers/v1/recipes/track_parcels/schedules/track_nightly.jadpo",
        "// Tracks every parcel in transit.\n",
    );
    dir
}

#[test]
fn a_recipe_is_copied_into_a_domain() {
    let dir = app("recipe-copied");
    let root = dir.path();
    let copied = jadpo(
        root,
        &[
            "package",
            "recipe",
            "couriers",
            "track_parcels",
            "--into",
            "shipping",
        ],
    );
    assert_eq!(copied.code, 0, "{}{}", copied.stdout, copied.stderr);
    assert_eq!(
        copied.stdout,
        "Copied recipe track_parcels into domains/shipping
    domains/shipping/actions/track_parcel.jadpo
    domains/shipping/schedules/track_nightly.jadpo

recipe copied: Couriers.v1, track_parcels, 2 files

next: jadpo check  shows what to fit to the app
"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("domains/shipping/actions/track_parcel.jadpo"))
            .expect("copied"),
        "// Tracks one parcel.\n"
    );
    // Again, it would write over the app's own files: nothing is copied.
    std::fs::remove_file(root.join("domains/shipping/schedules/track_nightly.jadpo"))
        .expect("remove one");
    let refused = jadpo(
        root,
        &[
            "package",
            "recipe",
            "couriers",
            "track_parcels",
            "--into",
            "shipping",
        ],
    );
    assert_eq!(refused.code, 2, "{}", refused.stdout);
    assert!(
        refused
            .stderr
            .starts_with("error: `domains/shipping/actions/track_parcel.jadpo` already exists\n"),
        "{}",
        refused.stderr
    );
    assert!(
        !root
            .join("domains/shipping/schedules/track_nightly.jadpo")
            .exists(),
        "all or nothing"
    );
}

#[test]
fn a_recipe_command_names_what_it_cant_find() {
    let dir = app("recipe-unknown");
    let root = dir.path();
    let cases: [(&[&str], &str); 4] = [
        (
            &[
                "package",
                "recipe",
                "couriers",
                "track_parcel",
                "--into",
                "shipping",
            ],
            "error: `Couriers.v1` has no recipe `track_parcel`\n  reason: `Couriers.v1` ships `track_parcels`. Did you mean `track_parcels`?\n  next: Use `jadpo package recipe couriers track_parcels --into shipping` (agent)\n",
        ),
        (
            &[
                "package",
                "recipe",
                "courier",
                "track_parcels",
                "--into",
                "shipping",
            ],
            "error: `courier` isn't a package in this project\n",
        ),
        (
            &[
                "package",
                "recipe",
                "couriers",
                "track_parcels",
                "--into",
                "shiping",
            ],
            "error: `shiping` isn't a domain of the app\n  reason: The app's domains are `shipping`. Did you mean `shipping`?\n",
        ),
        (
            &["package", "recipe", "couriers", "track_parcels"],
            "error: `jadpo package recipe` needs `--into <domain>`\n",
        ),
    ];
    for (args, starts) in cases {
        let out = jadpo(root, args);
        assert_eq!(out.code, 2, "{args:?}: {}", out.stdout);
        assert!(out.stderr.starts_with(starts), "{args:?}: {}", out.stderr);
    }
    assert!(!root.join("domains/shipping/actions").exists());
}

// --- recipes compiled in `sample/` (chunk 7.6b) ------------------------------

/// A package's own repository, the provider package `Couriers.v1` with
/// its `sample/` app and the recipe `track_parcels`, whose action is
/// `action`.
fn repository(name: &str, action: &str) -> tempdir::Dir {
    let dir = tempdir::Dir::new(name);
    let root = dir.path();
    write(
        root,
        "package.jadpo",
        "/// Parcel tracking.\npackage Couriers.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    );
    write(
        root,
        "provider.jadpo",
        "/// The couriers' tracking API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.couriers.example\"\n        sandbox: \"https://sandbox.couriers.example\"\n    }\n    credential: CredentialScheme.bearer\n}\n",
    );
    write(
        root,
        "types/parcel_code.jadpo",
        "/// A parcel's tracking code.\npublic type ParcelCode = Text {\n    max_length: 20\n}\n",
    );
    write(
        root,
        "types/track_request.jadpo",
        "/// Which parcel to track.\npublic type TrackRequest = Object {\n    code: ParcelCode\n}\n",
    );
    write(
        root,
        "types/parcel_status.jadpo",
        "/// Where a parcel is.\npublic type ParcelStatus = Object {\n    code: ParcelCode\n    delivered: Bool\n}\n",
    );
    write(
        root,
        "resources/parcels.jadpo",
        "public resource parcels {\n    track: {\n        method: HttpMethod.post\n        path: \"/v1/parcels/track\"\n        input: TrackRequest\n        output: ParcelStatus\n        source: PackageSource.written\n    }\n}\n",
    );
    write(
        root,
        "sample/app.jadpo",
        "application {\n    connections: {\n        couriers: {\n            package: Couriers.v1\n            domains: [shipping]\n            credential: config.couriers_key\n            environment: config.couriers_environment\n        }\n    }\n}\n",
    );
    write(
        root,
        "sample/config.jadpo",
        "config {\n    couriers_environment: ProviderEnvironment\n    couriers_key: Text {\n        secret: true\n    }\n}\n",
    );
    write(
        root,
        "sample/domains/shipping/types/note.jadpo",
        "/// A note about a parcel.\ntype Note = Text {\n    max_length: 200\n}\n",
    );
    write(
        root,
        "sample/domains/shipping/actions/noted.jadpo",
        "use shipping/types { Note }\n\n/// Gives a note back as it was written.\naction noted(note: Note) -> Note {\n    return note\n}\n",
    );
    write(
        root,
        "recipes/track_parcels/actions/track_parcel.jadpo",
        action,
    );
    dir
}

const TRACK: &str = "use app { couriers }\nuse shared/providers/couriers\n\n/// Tracks one parcel.\naction track_parcel(code: Couriers.ParcelCode) -> Couriers.ParcelStatus {\n    return couriers.parcels.track(Couriers.TrackRequest {\n        code: code\n    })\n}\n";

/// The publishing bar's row `item`, from `jadpo package check --json`.
fn bar_row(root: &std::path::Path, item: &str) -> serde_json::Value {
    let checked = jadpo(root, &["package", "check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&checked.stdout)
        .unwrap_or_else(|_| panic!("JSON: {}{}", checked.stdout, checked.stderr));
    document["data"]["bar"]
        .as_array()
        .and_then(|bar| bar.iter().find(|row| row["item"] == item))
        .cloned()
        .unwrap_or_else(|| panic!("a {item} row: {document}"))
}

#[test]
fn each_recipe_compiles_in_the_sample_app() {
    let dir = repository("recipe-sample", TRACK);
    let row = bar_row(dir.path(), "recipes");
    assert_eq!(row["state"], "passed", "{row}");
    assert_eq!(row["detail"], "1 recipe compiles in sample/");
}

#[test]
fn a_recipe_that_doesnt_compile_is_shown_at_its_own_path() {
    let broken = TRACK.replace("parcels.track", "parcels.trace");
    let dir = repository("recipe-broken", &broken);
    let row = bar_row(dir.path(), "recipes");
    assert_eq!(row["state"], "failed", "{row}");
    assert_eq!(row["detail"], "track_parcels doesn't compile");
    let checked = jadpo(dir.path(), &["package", "check"]);
    assert_eq!(checked.code, 1);
    assert!(
        checked.stdout.contains(
            "error: `couriers` has no operation `parcels.trace`\n --> recipes/track_parcels/actions/track_parcel.jadpo:"
        ),
        "{}",
        checked.stdout
    );
}

#[test]
fn recipes_with_no_sample_app_fail_the_bar() {
    let dir = repository("recipe-no-sample", TRACK);
    std::fs::remove_dir_all(dir.path().join("sample")).expect("remove the sample");
    let row = bar_row(dir.path(), "recipes");
    assert_eq!(row["state"], "failed", "{row}");
    assert_eq!(row["detail"], "no sample/ app");
    let checked = jadpo(dir.path(), &["package", "check"]);
    assert!(
        checked
            .stdout
            .starts_with("error: `Couriers.v1`'s recipes have no sample app to compile in\n"),
        "{}",
        checked.stdout
    );
    assert!(checked.stdout.contains("  rule: publish.sample_missing\n"));
}
