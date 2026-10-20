//! Library packages' translations from the command line (area 10, "Jadpo's
//! and packages' text"; area 11; chunk 7.1b3): an app's tests read a
//! package's messages in the app's wording, then the package's, then the
//! fallbacks; `jadpo translations update` records the app's translations
//! of them; `jadpo package show` lists them; and `jadpo package check`
//! holds every language a package ships to completeness.

use std::path::Path;

use jadpo_cli::{Output, run};
use jadpo_db::{Locked, Source, package_digest, write_lock};
use jadpo_term::Environment;

use super::http::{jadpo, tempdir, write};

const INDEX: &str = "/// UK postcodes: reading them as people write them.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: \"1.0\"
}
";

const HINT: &str = "/// Shown under a postcode field.
public type PostcodeHint = Message {}
";

const FOUND: &str = "/// How many postcodes a search found.
public type PostcodesFound = Message {
    count: PluralCount
}
";

const EN: &str = "translations {
    postcode_hint: \"Write it as it is on a letter, such as SW1A 1AA.\"
    postcodes_found: {
        one: \"{count} postcode\"
        other: \"{count} postcodes\"
    }
}
";

fn package_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("package.jadpo", INDEX),
        ("translations/postcode_hint.jadpo", HINT),
        ("translations/postcodes_found.jadpo", FOUND),
        ("translations/en_gb.jadpo", EN),
    ]
}

/// An app supporting British English, German and, in progress, French,
/// with the package installed, its German and a British rewording of the
/// hint in the root file.
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
            version: "2.0.0".to_string(),
            path: "../postcodes".to_string(),
            digest: package_digest(&held),
        },
    }]);
    write(&root, "jadpo.lock", &lock);
    write(
        &root,
        "domains/intake/types/postcode_count.jadpo",
        "/// How many postcodes.
type PostcodeCount = Int {
    min: 0
}
",
    );
    write(
        &root,
        "domains/intake/actions/postcode_text.jadpo",
        "use packages/postcodes

/// What the postcode field says, in the reader's language.
action postcode_text(reader_locale: Locale) -> PresentationText {
    return Postcodes.translations.postcode_hint(locale: reader_locale)
}
",
    );
    write(
        &root,
        "domains/intake/actions/found_text.jadpo",
        "use intake/types { PostcodeCount }
use packages/postcodes

/// How many postcodes a search found, in the reader's language.
action found_text(found: PostcodeCount, reader_locale: Locale) -> PresentationText {
    return Postcodes.translations.postcodes_found(count: found, locale: reader_locale)
}
",
    );
    write(
        &root,
        "domains/intake/actions/postcode_text.test.jadpo",
        "test \"the app's wording comes first, then the package's\" {
    assert postcode_text(Locale.en_gb) == \"Type it as it is on an envelope.\"
    assert postcode_text(Locale.de_de) == \"Schreiben Sie sie wie auf einem Brief.\"
}

test \"a locale in progress falls back to the default\" {
    assert postcode_text(Locale.fr_fr) == \"Type it as it is on an envelope.\"
}
",
    );
    write(
        &root,
        "domains/intake/actions/found_text.test.jadpo",
        "test \"the package's plural forms, and the app's\" {
    assert found_text(1, Locale.en_gb) == \"1 postcode\"
    assert found_text(1234, Locale.en_gb) == \"1,234 postcodes\"
    assert found_text(1234, Locale.de_de) == \"1.234 Postleitzahlen\"
    assert found_text(2, Locale.fr_fr) == \"2 postcodes\"
}
",
    );
    write(
        &root,
        "translations/en_gb.jadpo",
        "translations {
    postcodes: {
        postcode_hint: \"Type it as it is on an envelope.\"
    }
}
",
    );
    write(
        &root,
        "translations/de_de.jadpo",
        "translations {
    postcodes: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief.\"
        postcodes_found: {
            one: \"{count} Postleitzahl\"
            other: \"{count} Postleitzahlen\"
        }
    }
}
",
    );
    (dir, root)
}

#[test]
fn an_app_s_tests_read_a_package_s_messages_in_its_wording_then_the_package_s() {
    let (_dir, root) = app("package-words-test");
    let check = jadpo(&root, &["check"]);
    assert_eq!(check.code, 0, "{}{}", check.stdout, check.stderr);
    let test = jadpo(&root, &["test"]);
    assert_eq!(test.code, 0, "{}{}", test.stdout, test.stderr);
    assert!(test.stdout.contains("3 passed"), "{}", test.stdout);
    // French is in progress: what it falls back on is listed under the
    // root file.
    let missing = jadpo(&root, &["translations", "missing", "fr_fr"]);
    assert_eq!(missing.code, 0, "{}{}", missing.stdout, missing.stderr);
    assert!(
        missing.stdout.starts_with(
            "translations/fr_fr.jadpo  no file yet
  postcodes.postcode_hint    missing     falls back to en_gb
  postcodes.postcodes_found  missing     falls back to en_gb
"
        ),
        "{}",
        missing.stdout
    );
}

#[test]
fn update_records_the_app_s_translations_of_a_package_s_text() {
    let (_dir, root) = app("package-words-update");
    let update = jadpo(&root, &["translations", "update"]);
    assert_eq!(update.code, 0, "{}{}", update.stdout, update.stderr);
    let record = std::fs::read_to_string(root.join("translations/sources.jadpo_record"))
        .expect("the root record");
    let lines: Vec<&str> = record
        .lines()
        .filter(|line| line.starts_with("translation "))
        .map(|line| line.split(" from ").next().unwrap_or(line))
        .collect();
    assert_eq!(
        lines,
        [
            "translation de_de.postcodes.postcode_hint",
            "translation de_de.postcodes.postcodes_found",
        ],
        "{record}"
    );
    // The package rewords its English plural, which the app doesn't
    // reword; the app's German of it is stale.
    let reworded = EN.replace("\"{count} postcodes\"", "\"{count} UK postcodes\"");
    write(
        &root,
        "packages/postcodes/v2/translations/en_gb.jadpo",
        &reworded,
    );
    let mut files = package_files();
    files.retain(|(path, _)| *path != "translations/en_gb.jadpo");
    let mut held: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(path, text)| (*path, text.as_bytes()))
        .collect();
    held.push(("translations/en_gb.jadpo", reworded.as_bytes()));
    let lock = write_lock(&[Locked {
        package: "Postcodes.v2".to_string(),
        source: Source::Path {
            version: "2.0.1".to_string(),
            path: "../postcodes".to_string(),
            digest: package_digest(&held),
        },
    }]);
    write(&root, "jadpo.lock", &lock);
    let check = jadpo(&root, &["check"]);
    assert_eq!(check.code, 1, "{}", check.stdout);
    let all = format!("{}{}", check.stdout, check.stderr);
    assert!(
        all.contains(
            "`postcodes.postcodes_found` in `de_de` was translated from `en_gb` wording that has since changed"
        ),
        "{all}"
    );
}

#[test]
fn show_lists_a_package_s_messages_and_locales() {
    let (_dir, root) = app("package-words-show");
    let done = jadpo(&root, &["package", "show", "postcodes"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert_eq!(
        done.stdout,
        "UK postcodes: reading them as people write them.
  package: Postcodes.v2
  kind: library
  version: 2.0.0
  source: path ../postcodes
  licence: mit
  jadpo: 1.0
  locales: en_gb
  folder: packages/postcodes/v2

Messages
  translations.postcode_hint    ()
  translations.postcodes_found  (count: PluralCount)

Postcodes.v2: 2 messages

next: jadpo package show postcodes translations.postcode_hint  shows one in full, with its file
"
    );
    let one = jadpo(
        &root,
        &[
            "package",
            "show",
            "postcodes",
            "translations.postcodes_found",
        ],
    );
    assert_eq!(one.code, 0, "{}{}", one.stdout, one.stderr);
    assert_eq!(
        one.stdout,
        "How many postcodes a search found.
  name: translations.postcodes_found
  kind: message
  package: Postcodes.v2
  file: packages/postcodes/v2/translations/postcodes_found.jadpo:2
  takes: count: PluralCount, locale
  returns: PresentationText
  worded in: en_gb
"
    );
}

// --- jadpo package check ---------------------------------------------------

fn plain(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

#[test]
fn check_holds_every_language_a_package_ships_to_completeness() {
    let dir = tempdir::Dir::new("package-words-check");
    let root = dir.path().join("postcodes");
    for (path, text) in package_files() {
        write(&root, path, text);
    }
    // The changelog says why, so the bar's other rows decide the outcome.
    write(
        &root,
        "CHANGELOG.md",
        "# Changelog\n\n## 2.0.0\n\n- first release\n\nUK postcodes, read as people type them.\n",
    );
    let passed = plain(&root, &["package", "check"]);
    assert_eq!(passed.code, 0, "{}{}", passed.stdout, passed.stderr);
    assert!(
        passed.stdout.contains(
            "  described     passed                        every public declaration
  wording       passed                        every message in en_gb
"
        ),
        "{}",
        passed.stdout
    );
    let surface = std::fs::read_to_string(root.join("surface.jadpo_record")).expect("surface");
    assert!(surface.contains("    locale en_gb\n"), "{surface}");
    assert!(
        surface.contains("message translations.postcodes_found\n"),
        "{surface}"
    );

    // French that leaves a message out, and a message with no description.
    write(
        &root,
        "translations/fr_fr.jadpo",
        "translations {
    postcode_hint: \"Écrivez-le comme sur une lettre.\"
}
",
    );
    write(
        &root,
        "translations/postcode_hint.jadpo",
        "public type PostcodeHint = Message {}\n",
    );
    let failed = plain(&root, &["package", "check"]);
    assert_eq!(failed.code, 1, "{}{}", failed.stdout, failed.stderr);
    assert!(
        failed.stdout.contains(
            "  described     failed                        1 has no description
  wording       failed                        1 missing
"
        ),
        "{}",
        failed.stdout
    );
    let all = format!("{}{}", failed.stdout, failed.stderr);
    assert!(
        all.contains("Public message `PostcodeHint` has no description"),
        "{all}"
    );
    assert!(
        all.contains("`postcodes_found` has no wording in `translations/fr_fr.jadpo`"),
        "{all}"
    );
}
