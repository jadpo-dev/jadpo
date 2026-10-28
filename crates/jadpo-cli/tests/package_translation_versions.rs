//! Two versions of a library package with translations, from the command
//! line (area 11, "Two versions side by side, lazy loading and the package cache"): the
//! app words each under `<folder>_<version>`, its tests read each
//! version's wording, and `jadpo translations missing`, `export`, `import`,
//! `unused` and `update` name each version's text by that key. When a
//! second version is added, the app's wording moves to the earlier
//! version's key, and `update` carries its record lines with it.

use std::path::Path;

use jadpo_db::{Locked, Source, package_digest, write_lock};

use super::http::{jadpo, tempdir, write};

const V2_ENGLISH: &str = "Write it as it is on a letter, such as SW1A 1AA.";
const V3_ENGLISH: &str = "Write it as it is on a letter, spaces and all.";

/// A version's files, relative to its folder.
fn package_files(version: &str) -> Vec<(String, String)> {
    let english = if version == "v2" {
        V2_ENGLISH
    } else {
        V3_ENGLISH
    };
    vec![
        (
            "package.jadpo".to_string(),
            format!(
                "/// UK postcodes: reading them as people write them.
package Postcodes.{version} {{
    licence: Licence.mit
    jadpo: \"1.0\"
}}
"
            ),
        ),
        (
            "translations/postcode_hint.jadpo".to_string(),
            "/// Shown under a postcode field.
public type PostcodeHint = Message {}
"
            .to_string(),
        ),
        (
            "translations/en_gb.jadpo".to_string(),
            format!("translations {{\n    postcode_hint: \"{english}\"\n}}\n"),
        ),
    ]
}

/// Installs the versions in `held` and locks them.
fn hold(root: &Path, held: &[&str]) {
    let mut lines = Vec::new();
    for &version in held {
        let files = package_files(version);
        for (path, text) in &files {
            write(root, &format!("packages/postcodes/{version}/{path}"), text);
        }
        let bytes: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_bytes()))
            .collect();
        lines.push(Locked {
            package: format!("Postcodes.{version}"),
            source: Source::Path {
                version: format!("{}.0.0", &version[1..]),
                path: format!("../postcodes-{version}"),
                digest: package_digest(&bytes),
            },
        });
    }
    write(root, "jadpo.lock", &write_lock(&lines));
}

/// An app supporting British English, German and, in progress, French,
/// holding both versions, calling each one's hint, and wording both in
/// German.
fn app(name: &str) -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new(name);
    let root = dir.path().join("shop");
    write(
        &root,
        "app.jadpo",
        "application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.de_de, Locale.fr_fr]
    in_progress: [Locale.fr_fr]
}
",
    );
    hold(&root, &["v2", "v3"]);
    write(
        &root,
        "domains/intake/actions/old_hint.jadpo",
        "use packages/postcodes

/// The hint as callers showed it before the upgrade.
action old_hint(reader_locale: Locale) -> PresentationText {
    return Postcodes.v2.translations.postcode_hint(locale: reader_locale)
}
",
    );
    write(
        &root,
        "domains/intake/actions/new_hint.jadpo",
        "use packages/postcodes

/// The hint with the next version.
action new_hint(reader_locale: Locale) -> PresentationText {
    return Postcodes.v3.translations.postcode_hint(locale: reader_locale)
}
",
    );
    write(
        &root,
        "domains/intake/actions/old_hint.test.jadpo",
        "test \"the earlier version is worded under its key\" {
    assert old_hint(Locale.en_gb) == \"Write it as it is on a letter, such as SW1A 1AA.\"
    assert old_hint(Locale.de_de) == \"Schreiben Sie sie wie auf einem Brief.\"
}
",
    );
    write(
        &root,
        "domains/intake/actions/new_hint.test.jadpo",
        "test \"the next version is worded under its key\" {
    assert new_hint(Locale.en_gb) == \"Write it as it is on a letter, spaces and all.\"
    assert new_hint(Locale.de_de) == \"Schreiben Sie sie wie auf einem Brief, mit Leerzeichen.\"
}
",
    );
    write(
        &root,
        "translations/de_de.jadpo",
        "translations {
    postcodes_v2: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief.\"
    }
    postcodes_v3: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief, mit Leerzeichen.\"
    }
}
",
    );
    (dir, root)
}

#[test]
fn an_app_s_tests_read_each_version_s_wording() {
    let (_dir, root) = app("package-versions-test");
    let check = jadpo(&root, &["check"]);
    assert_eq!(check.code, 0, "{}{}", check.stdout, check.stderr);
    let test = jadpo(&root, &["test"]);
    assert_eq!(test.code, 0, "{}{}", test.stdout, test.stderr);
    assert!(test.stdout.contains("2 passed"), "{}", test.stdout);
    let missing = jadpo(&root, &["translations", "missing", "fr_fr"]);
    assert_eq!(missing.code, 0, "{}{}", missing.stdout, missing.stderr);
    assert!(
        missing.stdout.starts_with(
            "translations/fr_fr.jadpo  no file yet
  postcodes_v2.postcode_hint  missing     falls back to en_gb
  postcodes_v3.postcode_hint  missing     falls back to en_gb
"
        ),
        "{}",
        missing.stdout
    );
}

#[test]
fn export_and_import_name_each_version_s_text_by_its_key() {
    let (_dir, root) = app("package-versions-exchange");
    let done = jadpo(&root, &["translations", "export", "de_de"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    let xliff =
        std::fs::read_to_string(root.join("build/translations/de_de.xlf")).expect("the export");
    assert!(
        xliff.contains("<file id=\"postcodes_v2\" original=\"translations\">"),
        "{xliff}"
    );
    assert!(
        xliff.contains("<file id=\"postcodes_v3\" original=\"translations\">"),
        "{xliff}"
    );
    assert!(
        xliff.contains(&format!(
            "<source xml:space=\"preserve\">{V3_ENGLISH}</source>"
        )),
        "{xliff}"
    );

    let file = root.join("fr_fr.json");
    std::fs::write(
        &file,
        r#"{ "postcodes_v3.postcode_hint": "Écrivez-le comme sur une lettre, avec les espaces." }"#,
    )
    .expect("write");
    let done = jadpo(&root, &["translations", "import", "fr_fr.json"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert!(
        done.stdout.contains("postcodes_v3.postcode_hint  added"),
        "{}",
        done.stdout
    );
    assert_eq!(
        std::fs::read_to_string(root.join("translations/fr_fr.jadpo")).expect("written"),
        "translations {
    postcodes_v3: {
        postcode_hint: \"Écrivez-le comme sur une lettre, avec les espaces.\"
    }
}
"
    );
    let check = jadpo(&root, &["check"]);
    assert_eq!(check.code, 0, "{}{}", check.stdout, check.stderr);
}

#[test]
fn unused_names_the_version_a_member_isn_t_in() {
    let (_dir, root) = app("package-versions-unused");
    write(
        &root,
        "translations/de_de.jadpo",
        "translations {
    postcodes_v2: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief.\"
    }
    postcodes_v3: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief, mit Leerzeichen.\"
        street_hint: \"Die Straße, ohne Hausnummer.\"
    }
}
",
    );
    let done = jadpo(&root, &["translations", "unused"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert_eq!(
        done.stdout,
        "Package wording
  postcodes_v3.street_hint  translations/de_de.jadpo:7  not in Postcodes.v3

1 unused: 1 package wording
"
    );
}

#[test]
fn adding_a_second_version_moves_the_wording_and_its_record_lines() {
    let (_dir, root) = app("package-versions-move");
    // Before the upgrade: v2 alone, worded and accepted under the folder's
    // name.
    std::fs::remove_dir_all(root.join("packages/postcodes/v3")).expect("remove v3");
    hold(&root, &["v2"]);
    std::fs::remove_file(root.join("domains/intake/actions/new_hint.jadpo")).expect("remove");
    std::fs::remove_file(root.join("domains/intake/actions/new_hint.test.jadpo")).expect("remove");
    std::fs::remove_file(root.join("domains/intake/actions/old_hint.test.jadpo")).expect("remove");
    write(
        &root,
        "domains/intake/actions/old_hint.jadpo",
        "use packages/postcodes

/// The hint.
action old_hint(reader_locale: Locale) -> PresentationText {
    return Postcodes.translations.postcode_hint(locale: reader_locale)
}
",
    );
    write(
        &root,
        "translations/de_de.jadpo",
        "translations {
    postcodes: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief.\"
    }
}
",
    );
    let check = jadpo(&root, &["check"]);
    assert_eq!(check.code, 0, "{}{}", check.stdout, check.stderr);
    let accept = jadpo(&root, &["translations", "accept", "de_de"]);
    assert_eq!(accept.code, 0, "{}{}", accept.stdout, accept.stderr);
    let record = std::fs::read_to_string(root.join("translations/sources.jadpo_record"))
        .expect("the record");
    assert!(
        record.contains("translation de_de.postcodes.postcode_hint from ")
            && record.contains(" accepted"),
        "{record}"
    );

    // v3 is added beside it: the folder's key no longer says which.
    hold(&root, &["v2", "v3"]);
    write(
        &root,
        "domains/intake/actions/old_hint.jadpo",
        "use packages/postcodes

/// The hint.
action old_hint(reader_locale: Locale) -> PresentationText {
    return Postcodes.v2.translations.postcode_hint(locale: reader_locale)
}
",
    );
    let check = jadpo(&root, &["check"]);
    assert_eq!(check.code, 1, "{}{}", check.stdout, check.stderr);
    let all = format!("{}{}", check.stdout, check.stderr);
    assert!(
        all.contains("`postcodes` words package `Postcodes`, which is held in two versions"),
        "{all}"
    );
    assert!(all.contains("Rename the group to `postcodes_v2`"), "{all}");

    // The group renamed as the check says, and the new version worded:
    // the earlier version's line follows its wording, still accepted.
    write(
        &root,
        "translations/de_de.jadpo",
        "translations {
    postcodes_v2: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief.\"
    }
    postcodes_v3: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief, mit Leerzeichen.\"
    }
}
",
    );
    let check = jadpo(&root, &["check"]);
    assert_eq!(check.code, 0, "{}{}", check.stdout, check.stderr);
    let update = jadpo(&root, &["translations", "update"]);
    assert_eq!(update.code, 0, "{}{}", update.stdout, update.stderr);
    assert!(
        update.stdout.contains(
            "de_de.postcodes_v2.postcode_hint  moved with its wording to the package's new key"
        ),
        "{}",
        update.stdout
    );
    let record = std::fs::read_to_string(root.join("translations/sources.jadpo_record"))
        .expect("the record");
    let lines: Vec<&str> = record
        .lines()
        .filter(|line| line.starts_with("translation "))
        .collect();
    assert_eq!(lines.len(), 2, "{record}");
    assert!(
        lines[0].starts_with("translation de_de.postcodes_v2.postcode_hint from ")
            && lines[0].ends_with(" accepted"),
        "{record}"
    );
    assert!(
        lines[1].starts_with("translation de_de.postcodes_v3.postcode_hint from ")
            && !lines[1].ends_with(" accepted"),
        "{record}"
    );
}
