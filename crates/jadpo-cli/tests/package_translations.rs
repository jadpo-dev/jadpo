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

// --- jadpo translations export, import and unused (chunk 7.1b4) ------------

/// Every file under `root` and what is in it, to see that nothing moved.
fn snapshot(root: &Path) -> Vec<(String, String)> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<(String, String)>) {
        let mut entries: Vec<_> = std::fs::read_dir(at)
            .expect("read_dir")
            .filter_map(Result::ok)
            .collect();
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let shown = path
                    .strip_prefix(root)
                    .expect("inside")
                    .to_string_lossy()
                    .to_string();
                out.push((shown, std::fs::read_to_string(&path).unwrap_or_default()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

/// The package with its own German wording of the hint, installed again.
fn with_package_german(root: &Path) -> &'static str {
    const DE: &str = "translations {
    postcode_hint: \"Schreiben Sie sie wie auf einem Umschlag.\"
    postcodes_found: {
        one: \"{count} Postleitzahl\"
        other: \"{count} Postleitzahlen\"
    }
}
";
    write(root, "packages/postcodes/v2/translations/de_de.jadpo", DE);
    let mut files = package_files();
    files.push(("translations/de_de.jadpo", DE));
    let held: Vec<(&str, &[u8])> = files
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
    write(root, "jadpo.lock", &lock);
    DE
}

#[test]
fn export_sends_a_package_s_text_under_its_folder_name() {
    let (_dir, root) = app("package-words-export");
    let done = jadpo(&root, &["translations", "export", "de_de"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert_eq!(
        done.stdout,
        "Wrote build/translations/de_de.xlf
    de_de  3 units, 3 translated

Exported 1 locale: 3 units, xliff

next: jadpo translations import build/translations/de_de.xlf  reads the translations back
"
    );
    let xliff =
        std::fs::read_to_string(root.join("build/translations/de_de.xlf")).expect("the export");
    assert_eq!(
        xliff,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff xmlns="urn:oasis:names:tc:xliff:document:2.0" version="2.0" srcLang="en-GB" trgLang="de-DE">
  <file id="postcodes" original="translations">
    <unit id="postcode_hint">
      <notes>
        <note category="description">Shown under a postcode field.</note>
      </notes>
      <segment state="translated">
        <source xml:space="preserve">Type it as it is on an envelope.</source>
        <target xml:space="preserve">Schreiben Sie sie wie auf einem Brief.</target>
      </segment>
    </unit>
    <unit id="postcodes_found.one">
      <notes>
        <note category="description">How many postcodes a search found.</note>
        <note category="context">Used for the plural form `one` of `count`.</note>
        <note category="placeholders">{count}</note>
      </notes>
      <segment state="translated">
        <source xml:space="preserve">{count} postcode</source>
        <target xml:space="preserve">{count} Postleitzahl</target>
      </segment>
    </unit>
    <unit id="postcodes_found.other">
      <notes>
        <note category="description">How many postcodes a search found.</note>
        <note category="context">Used for the plural form `other` of `count`.</note>
        <note category="placeholders">{count}</note>
      </notes>
      <segment state="translated">
        <source xml:space="preserve">{count} postcodes</source>
        <target xml:space="preserve">{count} Postleitzahlen</target>
      </segment>
    </unit>
  </file>
</xliff>
"#
    );
    // French, in progress, has nothing yet: every unit is to translate.
    let json = jadpo(
        &root,
        &["translations", "export", "fr_fr", "--format", "json"],
    );
    assert_eq!(json.code, 0, "{}{}", json.stdout, json.stderr);
    let text = std::fs::read_to_string(root.join("build/translations/fr_fr.json")).expect("json");
    assert!(text.contains("\"postcodes.postcode_hint\": {"), "{text}");
    assert!(
        text.contains("\"postcodes.postcodes_found.other\": {"),
        "{text}"
    );
}

#[test]
fn import_writes_the_app_s_wording_of_a_package_s_text_in_the_root_file() {
    let (_dir, root) = app("package-words-import");
    let package_before: Vec<(String, String)> = snapshot(&root.join("packages"));
    let file = root.join("fr_fr.json");
    std::fs::write(
        &file,
        r#"{
  "postcodes.postcode_hint": { "message": "Écrivez-le comme sur une enveloppe." },
  "postcodes.postcodes_found.one": { "message": "{count} code postal" },
  "postcodes.postcodes_found.many": { "message": "{count} de codes postaux" },
  "postcodes.postcodes_found.other": { "message": "{count} codes postaux" }
}
"#,
    )
    .expect("write");
    let done = jadpo(&root, &["translations", "import", "fr_fr.json"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert_eq!(
        done.stdout,
        "Wrote translations/fr_fr.jadpo
    postcodes.postcode_hint          added
    postcodes.postcodes_found.one    added
    postcodes.postcodes_found.many   added
    postcodes.postcodes_found.other  added

Wrote translations/sources.jadpo_record

Imported fr_fr: 4 added

next: jadpo check  confirms the imported wording
"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("translations/fr_fr.jadpo")).expect("written"),
        "translations {
    postcodes: {
        postcode_hint: \"Écrivez-le comme sur une enveloppe.\"
        postcodes_found: {
            one: \"{count} code postal\"
            many: \"{count} de codes postaux\"
            other: \"{count} codes postaux\"
        }
    }
}
"
    );
    let record =
        std::fs::read_to_string(root.join("translations/sources.jadpo_record")).expect("record");
    assert!(
        record.contains("translation fr_fr.postcodes.postcode_hint from "),
        "{record}"
    );
    // The package's own files are the package's: a translator never
    // changes them.
    assert_eq!(snapshot(&root.join("packages")), package_before);
    let check = jadpo(&root, &["check"]);
    assert_eq!(check.code, 0, "{}{}", check.stdout, check.stderr);

    // Reading the same file again changes nothing.
    let before = snapshot(&root);
    let again = jadpo(&root, &["translations", "import", "fr_fr.json"]);
    assert_eq!(again.code, 0, "{}{}", again.stdout, again.stderr);
    assert_eq!(again.stdout, "Already up to date: fr_fr, 4 unchanged\n");
    assert_eq!(snapshot(&root), before);
}

#[test]
fn a_package_s_own_wording_goes_out_and_a_changed_one_comes_back_as_the_app_s() {
    let (_dir, root) = app("package-words-own");
    with_package_german(&root);
    // The app words none of German itself now: the package does.
    std::fs::remove_file(root.join("translations/de_de.jadpo")).expect("remove");
    let check = jadpo(&root, &["check"]);
    assert_eq!(check.code, 0, "{}{}", check.stdout, check.stderr);
    let export = jadpo(&root, &["translations", "export", "de_de"]);
    assert_eq!(export.code, 0, "{}{}", export.stdout, export.stderr);
    assert!(
        export.stdout.contains("de_de  3 units, 3 translated"),
        "{}",
        export.stdout
    );
    let xliff_path = root.join("build/translations/de_de.xlf");
    let xliff = std::fs::read_to_string(&xliff_path).expect("the export");
    assert!(
        xliff.contains(
            "<target xml:space=\"preserve\">Schreiben Sie sie wie auf einem Umschlag.</target>"
        ),
        "{xliff}"
    );
    // Exported and read back unchanged: nothing is written.
    let before = snapshot(&root);
    let same = jadpo(
        &root,
        &["translations", "import", "build/translations/de_de.xlf"],
    );
    assert_eq!(same.code, 0, "{}{}", same.stdout, same.stderr);
    assert_eq!(same.stdout, "Already up to date: de_de, 3 unchanged\n");
    assert_eq!(snapshot(&root), before);
    // The translator rewords one plural form: the app words the whole
    // message in its root file, the package's file stays as it was.
    let package_file = root.join("packages/postcodes/v2/translations/de_de.jadpo");
    let package_before = std::fs::read_to_string(&package_file).expect("package file");
    std::fs::write(
        &xliff_path,
        xliff.replace(
            "<target xml:space=\"preserve\">{count} Postleitzahlen</target>",
            "<target xml:space=\"preserve\">{count} PLZ</target>",
        ),
    )
    .expect("write");
    let reworded = jadpo(
        &root,
        &["translations", "import", "build/translations/de_de.xlf"],
    );
    assert_eq!(reworded.code, 0, "{}{}", reworded.stdout, reworded.stderr);
    assert!(
        reworded.stdout.starts_with(
            "Wrote translations/de_de.jadpo
    postcodes.postcodes_found.other  reworded
"
        ),
        "{}",
        reworded.stdout
    );
    assert_eq!(
        std::fs::read_to_string(root.join("translations/de_de.jadpo")).expect("written"),
        "translations {
    postcodes: {
        postcodes_found: {
            one: \"{count} Postleitzahl\"
            other: \"{count} PLZ\"
        }
    }
}
"
    );
    assert_eq!(
        std::fs::read_to_string(&package_file).expect("package file"),
        package_before
    );
    let check = jadpo(&root, &["check"]);
    assert_eq!(check.code, 0, "{}{}", check.stdout, check.stderr);
}

#[test]
fn import_adds_a_package_s_group_beside_jadpo_s_in_one_root_file() {
    let (_dir, root) = app("package-words-groups");
    write(
        &root,
        "translations/fr_fr.jadpo",
        "translations {
    // Jadpo's own, worded by hand.
    jadpo: {
        text_direction: {
            left_to_right: \"ltr\"
            right_to_left: \"rtl\"
        }
    }
}
",
    );
    let file = root.join("fr_fr.json");
    std::fs::write(
        &file,
        r#"{ "postcodes.postcode_hint": "Écrivez-le comme sur une enveloppe." }"#,
    )
    .expect("write");
    let done = jadpo(&root, &["translations", "import", "fr_fr.json"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert_eq!(
        std::fs::read_to_string(root.join("translations/fr_fr.jadpo")).expect("written"),
        "translations {
    // Jadpo's own, worded by hand.
    jadpo: {
        text_direction: {
            left_to_right: \"ltr\"
            right_to_left: \"rtl\"
        }
    }
    postcodes: {
        postcode_hint: \"Écrivez-le comme sur une enveloppe.\"
    }
}
"
    );
}

#[test]
fn import_refuses_a_unit_a_package_does_not_have() {
    let (_dir, root) = app("package-words-unknown");
    let file = root.join("de_de.json");
    std::fs::write(
        &file,
        r#"{ "postcodes.postcode_hnt": "Schreiben Sie sie." }"#,
    )
    .expect("write");
    let before = snapshot(&root);
    let done = jadpo(&root, &["translations", "import", "de_de.json"]);
    assert_eq!(done.code, 1, "{}{}", done.stdout, done.stderr);
    let all = format!("{}{}", done.stdout, done.stderr);
    assert!(
        all.contains("`postcodes.postcode_hnt` isn't a unit of wording in this project"),
        "{all}"
    );
    assert!(
        all.contains("Did you mean `postcodes.postcode_hint`?"),
        "{all}"
    );
    assert_eq!(snapshot(&root), before);
}

#[test]
fn unused_lists_the_app_s_wording_of_package_text_nothing_shows() {
    let (_dir, root) = app("package-words-unused");
    // Nothing calls the count any more, and the app still words a message
    // the package no longer has.
    std::fs::remove_file(root.join("domains/intake/actions/found_text.jadpo")).expect("remove");
    std::fs::remove_file(root.join("domains/intake/actions/found_text.test.jadpo"))
        .expect("remove");
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
  postcodes.postcodes_found  translations/de_de.jadpo:4  no code calls it
  postcodes.street_hint      translations/de_de.jadpo:8  not in Postcodes

2 unused: 2 package wordings
"
    );
    let json = jadpo(&root, &["translations", "unused", "--json"]);
    assert_eq!(json.code, 0, "{}{}", json.stdout, json.stderr);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("json");
    let wording = &document["data"]["packageWording"];
    assert_eq!(wording[1]["name"], "postcodes.street_hint", "{wording}");
    assert_eq!(wording[1]["package"], "Postcodes", "{wording}");
    assert_eq!(wording[1]["reason"], "notInPackage", "{wording}");
    assert_eq!(
        wording[1]["place"], "translations/de_de.jadpo:8",
        "{wording}"
    );
    assert_eq!(wording[0]["reason"], "uncalled", "{wording}");

    // With the package's text all called and worded, nothing is unused.
    let (_dir, root) = app("package-words-unused-none");
    let done = jadpo(&root, &["translations", "unused"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert_eq!(done.stdout, "No messages\n");
}
