//! Two versions of a library package with translations, held side by side
//! (area 11, "Two versions side by side, lazy loading and the package
//! cache"): while the app holds two versions, it words each under a key
//! carrying the version (`postcodes_v3: { }`), as its names carry it;
//! with one held, the key is the folder's name. A group whose key no
//! longer fits what is held is named with the key to use, and the root
//! record's lines follow the wording to its new key.

use crate::support;

use jadpo_check::translations::sources::{Entry, hash, render};
use jadpo_db::{Database, Layout, Locked, Source, package_digest, write_lock};
use jadpo_diag::Diagnostic;
use support::project_with_records;

const APP: (&str, &str) = (
    "app.jadpo",
    "application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.de_de]
}
",
);

const V2_ENGLISH: &str = "Write it as it is on a letter, such as SW1A 1AA.";
const V3_ENGLISH: &str = "Write it as it is on a letter, spaces and all.";

/// A version of the package: its index, its hint and its English.
fn version(version: &str, english: &str) -> Vec<(String, String)> {
    let at = format!("packages/postcodes/{version}");
    vec![
        (
            format!("{at}/package.jadpo"),
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
            format!("{at}/translations/postcode_hint.jadpo"),
            "/// Shown under a postcode field.
public type PostcodeHint = Message {}
"
            .to_string(),
        ),
        (
            format!("{at}/translations/en_gb.jadpo"),
            format!("translations {{\n    postcode_hint: \"{english}\"\n}}\n"),
        ),
    ]
}

/// One action calling each version's hint.
const CALLS: [(&str, &str); 2] = [
    (
        "domains/intake/actions/old_hint.jadpo",
        "use packages/postcodes

/// The hint as callers showed it before the upgrade.
action old_hint(reader_locale: Locale) -> PresentationText {
    return Postcodes.v2.translations.postcode_hint(locale: reader_locale)
}
",
    ),
    (
        "domains/intake/actions/new_hint.jadpo",
        "use packages/postcodes

/// The hint with the next version.
action new_hint(reader_locale: Locale) -> PresentationText {
    return Postcodes.v3.translations.postcode_hint(locale: reader_locale)
}
",
    ),
];

/// The app's German of both versions.
const ROOT_DE: (&str, &str) = (
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

/// The check's diagnostics, with the versions in `held` installed and
/// locked, and `extra` files and records beside them.
fn check(held: &[&str], extra: &[(&str, &str)], records: &[(&str, &str)]) -> Vec<Diagnostic> {
    let mut files: Vec<(String, String)> = Vec::new();
    let mut lines = Vec::new();
    for &at in held {
        let english = if at == "v2" { V2_ENGLISH } else { V3_ENGLISH };
        let package = version(at, english);
        let prefix = format!("packages/postcodes/{at}/");
        let bytes: Vec<(&str, &[u8])> = package
            .iter()
            .filter_map(|(path, text)| Some((path.strip_prefix(&prefix)?, text.as_bytes())))
            .collect();
        lines.push(Locked {
            package: format!("Postcodes.{at}"),
            source: Source::Path {
                version: format!("{}.0.0", &at[1..]),
                path: format!("../postcodes-{at}"),
                digest: package_digest(&bytes),
            },
        });
        files.extend(package.clone());
    }
    let lock = write_lock(&lines);
    let mut all: Vec<(&str, &str)> = vec![APP];
    all.extend(files.iter().map(|(p, t)| (p.as_str(), t.as_str())));
    all.extend_from_slice(extra);
    let mut with_lock: Vec<(&str, &str)> = vec![("jadpo.lock", &lock)];
    with_lock.extend_from_slice(records);
    let db = Database::default();
    let project = project_with_records(&db, Layout::Project, &all, &with_lock);
    jadpo_check::check(&db, project)
}

fn lines(found: &[Diagnostic]) -> Vec<String> {
    found
        .iter()
        .map(|d| {
            let path = d
                .primary
                .as_ref()
                .map_or("", |label| label.span.path.as_str());
            format!("{path} {}: {}", d.rule.id, d.message)
        })
        .collect()
}

fn edits(found: &[Diagnostic]) -> Vec<String> {
    found
        .iter()
        .filter_map(|d| d.next.as_ref())
        .flat_map(|next| next.edits.iter().map(|edit| edit.replacement.clone()))
        .collect()
}

#[test]
fn each_version_is_worded_under_a_key_carrying_it() {
    let mut extra = CALLS.to_vec();
    extra.push(ROOT_DE);
    let found = check(&["v2", "v3"], &extra, &[]);
    assert_eq!(lines(&found), Vec::<String>::new());
}

#[test]
fn a_version_s_missing_wording_names_its_key() {
    let found = lines(&check(&["v2", "v3"], &CALLS, &[]));
    assert!(
        found.contains(
            &"app.jadpo translation.file_missing: `Locale.de_de` is supported but package `Postcodes.v2` has no wording in it for `postcode_hint`"
                .to_string()
        ),
        "{found:#?}"
    );
    assert!(
        found.contains(
            &"app.jadpo translation.file_missing: `Locale.de_de` is supported but package `Postcodes.v3` has no wording in it for `postcode_hint`"
                .to_string()
        ),
        "{found:#?}"
    );
    let partial = (
        "translations/de_de.jadpo",
        "translations {
    postcodes_v2: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief.\"
    }
}
",
    );
    let mut extra = CALLS.to_vec();
    extra.push(partial);
    let found = check(&["v2", "v3"], &extra, &[]);
    assert_eq!(
        lines(&found),
        vec![
            "translations/de_de.jadpo translation.wording_missing: `postcodes_v3.postcode_hint` has no wording in `translations/de_de.jadpo`"
                .to_string()
        ]
    );
    let next = found[0].next.as_ref().expect("a next step");
    assert_eq!(
        next.text,
        "Add `postcode_hint` to the `postcodes_v3` group of `translations/de_de.jadpo`"
    );
}

#[test]
fn the_folder_s_key_is_moved_to_the_earlier_version_once_a_second_is_held() {
    // The app worded v2 alone under the folder's name; v3 is added beside.
    let before = (
        "translations/de_de.jadpo",
        "translations {
    postcodes: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief.\"
    }
    postcodes_v3: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief, mit Leerzeichen.\"
    }
}
",
    );
    let mut extra = CALLS.to_vec();
    extra.push(before);
    let found = check(&["v2", "v3"], &extra, &[]);
    let unknown: Vec<&Diagnostic> = found
        .iter()
        .filter(|d| d.rule.id == "translation.group_unknown")
        .collect();
    assert_eq!(unknown.len(), 1, "{:#?}", lines(&found));
    assert_eq!(
        unknown[0].message,
        "`postcodes` words package `Postcodes`, which is held in two versions"
    );
    assert_eq!(edits(&found), vec!["postcodes_v2".to_string()]);
}

#[test]
fn a_version_s_key_goes_back_to_the_folder_s_once_one_is_held() {
    let after = (
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
    let call = (
        "domains/intake/actions/new_hint.jadpo",
        "use packages/postcodes

/// The hint.
action new_hint(reader_locale: Locale) -> PresentationText {
    return Postcodes.translations.postcode_hint(locale: reader_locale)
}
",
    );
    let found = check(&["v3"], &[call, after], &[]);
    let unknown: Vec<String> = found
        .iter()
        .filter(|d| d.rule.id == "translation.group_unknown")
        .map(|d| d.message.clone())
        .collect();
    assert_eq!(
        unknown,
        vec![
            "`postcodes_v2` words `Postcodes.v2`, which the app doesn't hold".to_string(),
            "`postcodes_v3` words package `Postcodes`, which is held in one version".to_string(),
        ],
        "{:#?}",
        lines(&found)
    );
    assert_eq!(edits(&found), vec!["postcodes".to_string()]);
}

#[test]
fn a_domain_can_t_take_a_version_s_key() {
    let domain = (
        "domains/postcodes_v3/actions/noop.jadpo",
        "/// Does nothing.\naction noop() -> Unit {}\n",
    );
    let mut extra = CALLS.to_vec();
    extra.push(ROOT_DE);
    extra.push(domain);
    let found = check(&["v2", "v3"], &extra, &[]);
    let taken: Vec<&Diagnostic> = found
        .iter()
        .filter(|d| d.rule.id == "package.name_taken")
        .collect();
    assert_eq!(taken.len(), 1, "{:#?}", lines(&found));
    assert!(
        taken[0]
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("domain `postcodes_v3`")),
        "{:#?}",
        taken[0]
    );
}

#[test]
fn the_record_s_line_follows_the_wording_to_its_new_key() {
    let german = "Schreiben Sie sie wie auf einem Brief.";
    // Recorded while v2 was held alone, under the folder's name.
    let record = render(&[Entry {
        locale: "de_de".to_string(),
        member: "postcodes.postcode_hint".to_string(),
        from: hash(V2_ENGLISH),
        wording: hash(german),
        accepted: true,
    }]);
    let moved = |key: &str| {
        (
            "translations/de_de.jadpo",
            format!(
                "translations {{
    {key}: {{
        postcode_hint: \"{german}\"
    }}
}}
"
            ),
        )
    };
    // Moved to v2's key: still made from v2's English, so not stale.
    let v2 = moved("postcodes_v2");
    let found = check(
        &["v2", "v3"],
        &[CALLS[0], (v2.0, &v2.1)],
        &[("translations/sources.jadpo_record", &record)],
    );
    assert!(
        !lines(&found)
            .iter()
            .any(|p| p.contains("translation.stale")),
        "{:#?}",
        lines(&found)
    );
    // Moved to v3's key instead: made from v2's English, which isn't
    // v3's, so stale.
    let v3 = moved("postcodes_v3");
    let found = check(
        &["v2", "v3"],
        &[CALLS[1], (v3.0, &v3.1)],
        &[("translations/sources.jadpo_record", &record)],
    );
    assert!(
        lines(&found).contains(
            &"translations/de_de.jadpo translation.stale: `postcodes_v3.postcode_hint` in `de_de` was translated from `en_gb` wording that has since changed"
                .to_string()
        ),
        "{:#?}",
        lines(&found)
    );
}
