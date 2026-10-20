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
