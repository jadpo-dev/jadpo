//! The exchange with translation services (area 10, "Tooling"; chunk
//! 5.4f): `jadpo translations export` writes every message, its description
//! and its wording as XLIFF 2.0 or JSON, and `import` reads the translations
//! back, checks them as the language files are checked, writes the language
//! files and the record, or refuses and writes nothing.
//!
//! The projects have no entities, so no database is needed.

use std::path::Path;

use super::http::{jadpo, tempdir, write};

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.de_de, Locale.pl_pl]\n}\n",
    ),
    (
        "domains/shipping/types/formality.jadpo",
        "/// How formally a customer is addressed.\ntype Formality = Enum {\n    formal\n    casual\n}\n",
    ),
    (
        "domains/shipping/translations/order_shipped.jadpo",
        "use shipping/types { Formality }\n\n/// Subject of the email sent when a parcel leaves the warehouse.\ntype OrderShipped = Message {\n    formality: Formality\n    name: Text\n}\n",
    ),
    (
        "domains/shipping/translations/parcels.jadpo",
        "/// How many parcels an order has, on the tracking page.\ntype Parcels = Message {\n    count: PluralCount\n}\n",
    ),
    (
        "domains/shipping/translations/farewell.jadpo",
        "/// Said when a customer signs out.\ntype Farewell = Message {}\n",
    ),
    (
        "domains/shipping/translations/en_gb.jadpo",
        "translations {\n    // Reviewed by Sam.\n    order_shipped: {\n        formal: \"Good day {name}, your order has shipped.\"\n        casual: \"Hi {name}, your order is on its way!\"\n    }\n    parcels: {\n        one: \"{count} parcel\"\n        other: \"{count} parcels\"\n    }\n    farewell: \"Goodbye.\"\n}\n",
    ),
    (
        "domains/shipping/translations/de_de.jadpo",
        "translations {\n    // Reviewed by Jana.\n    order_shipped: {\n        formal: \"Guten Tag {name}, Ihre Bestellung wurde versandt.\"\n        casual: \"Hallo {name}, deine Bestellung ist unterwegs!\"\n    }\n    parcels: {\n        one: \"{count} Paket\"\n        other: \"{count} Pakete\"\n    }\n    farewell: \"Auf Wiedersehen.\"\n}\n",
    ),
];

const DE_DE: &str = "domains/shipping/translations/de_de.jadpo";
const PL_PL: &str = "domains/shipping/translations/pl_pl.jadpo";
const RECORD: &str = "domains/shipping/translations/sources.jadpo_record";

fn project(name: &str) -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new(name);
    let root = dir.path().join("shipping");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    (dir, root)
}

fn plain(root: &Path, args: &[&str]) -> (u8, String) {
    let done = jadpo(root, args);
    (done.code, format!("{}{}", done.stdout, done.stderr))
}

fn read(root: &Path, path: &str) -> String {
    std::fs::read_to_string(root.join(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// Every file of the project and what is in it, to see that nothing moved.
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

/// The Polish translations a service would send back, as XLIFF.
const POLISH: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff xmlns="urn:oasis:names:tc:xliff:document:2.0" version="2.0" srcLang="en-GB" trgLang="pl-PL">
  <file id="shipping">
    <unit id="farewell"><segment><source>Goodbye.</source><target>Do widzenia.</target></segment></unit>
    <unit id="order_shipped.formal"><segment><source>Good day {name}, your order has shipped.</source><target>Dzień dobry {name}, Twoje zamówienie zostało wysłane.</target></segment></unit>
    <unit id="order_shipped.casual"><segment><source>Hi {name}, your order is on its way!</source><target>Cześć {name}, Twoja paczka jest w drodze!</target></segment></unit>
    <unit id="parcels.one"><segment><source>{count} parcel</source><target>{count} paczka</target></segment></unit>
    <unit id="parcels.few"><segment><source>{count} parcels</source><target>{count} paczki</target></segment></unit>
    <unit id="parcels.many"><segment><source>{count} parcels</source><target>{count} paczek</target></segment></unit>
    <unit id="parcels.other"><segment><source>{count} parcels</source><target>{count} paczki</target></segment></unit>
  </file>
</xliff>
"#;

// --- export --------------------------------------------------------------------------

#[test]
fn export_writes_a_file_for_each_translated_locale_as_xliff() {
    let (_dir, root) = project("exchange-export");
    let (code, out) = plain(&root, &["translations", "export"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out,
        "Wrote build/translations/de_de.xlf
    de_de  5 units, 5 translated

Wrote build/translations/pl_pl.xlf
    pl_pl  7 units, 7 to translate

Exported 2 locales: 12 units, xliff

next: jadpo translations import build/translations/pl_pl.xlf  reads the translations back
"
    );
    assert_eq!(
        read(&root, "build/translations/de_de.xlf"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff xmlns="urn:oasis:names:tc:xliff:document:2.0" version="2.0" srcLang="en-GB" trgLang="de-DE">
  <file id="shipping" original="domains/shipping/translations">
    <unit id="farewell">
      <notes>
        <note category="description">Said when a customer signs out.</note>
      </notes>
      <segment state="translated">
        <source xml:space="preserve">Goodbye.</source>
        <target xml:space="preserve">Auf Wiedersehen.</target>
      </segment>
    </unit>
    <unit id="order_shipped.formal">
      <notes>
        <note category="description">Subject of the email sent when a parcel leaves the warehouse.</note>
        <note category="context">Used when `formality` is `formal`.</note>
        <note category="placeholders">{name}</note>
      </notes>
      <segment state="translated">
        <source xml:space="preserve">Good day {name}, your order has shipped.</source>
        <target xml:space="preserve">Guten Tag {name}, Ihre Bestellung wurde versandt.</target>
      </segment>
    </unit>
    <unit id="order_shipped.casual">
      <notes>
        <note category="description">Subject of the email sent when a parcel leaves the warehouse.</note>
        <note category="context">Used when `formality` is `casual`.</note>
        <note category="placeholders">{name}</note>
      </notes>
      <segment state="translated">
        <source xml:space="preserve">Hi {name}, your order is on its way!</source>
        <target xml:space="preserve">Hallo {name}, deine Bestellung ist unterwegs!</target>
      </segment>
    </unit>
    <unit id="parcels.one">
      <notes>
        <note category="description">How many parcels an order has, on the tracking page.</note>
        <note category="context">Used for the plural form `one` of `count`.</note>
        <note category="placeholders">{count}</note>
      </notes>
      <segment state="translated">
        <source xml:space="preserve">{count} parcel</source>
        <target xml:space="preserve">{count} Paket</target>
      </segment>
    </unit>
    <unit id="parcels.other">
      <notes>
        <note category="description">How many parcels an order has, on the tracking page.</note>
        <note category="context">Used for the plural form `other` of `count`.</note>
        <note category="placeholders">{count}</note>
      </notes>
      <segment state="translated">
        <source xml:space="preserve">{count} parcels</source>
        <target xml:space="preserve">{count} Pakete</target>
      </segment>
    </unit>
  </file>
</xliff>
"#
    );
    // Polish needs four plural forms; each is translated from English's
    // `other` where English has no form of that name.
    let polish = read(&root, "build/translations/pl_pl.xlf");
    assert!(polish.contains("trgLang=\"pl-PL\""), "{polish}");
    for form in ["one", "few", "many", "other"] {
        assert!(
            polish.contains(&format!("<unit id=\"parcels.{form}\">")),
            "{polish}"
        );
    }
    assert!(polish.contains("<segment state=\"initial\">"), "{polish}");
    assert!(!polish.contains("<target"), "{polish}");
}

#[test]
fn export_json_carries_the_description_beside_the_wording() {
    let (_dir, root) = project("exchange-export-json");
    let (code, out) = plain(
        &root,
        &[
            "translations",
            "export",
            "de_de",
            "--format",
            "json",
            "--out",
            "for-service",
        ],
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.starts_with("Wrote for-service/de_de.json\n"), "{out}");
    let text = read(&root, "for-service/de_de.json");
    let document: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(
        document["shipping.farewell"],
        serde_json::json!({
            "description": "Said when a customer signs out.",
            "message": "Auf Wiedersehen."
        })
    );
    assert_eq!(
        document["shipping.order_shipped.casual"]["description"],
        "Subject of the email sent when a parcel leaves the warehouse.\nUsed when `formality` is `casual`.\n{name}"
    );
    assert_eq!(document.as_object().expect("object").len(), 5);
}

#[test]
fn export_of_the_default_locale_is_the_source_to_upload() {
    let (_dir, root) = project("exchange-export-source");
    let (code, out) = plain(&root, &["translations", "export", "en_gb"]);
    assert_eq!(code, 0, "{out}");
    let text = read(&root, "build/translations/en_gb.xlf");
    assert!(text.contains("srcLang=\"en-GB\">"), "{text}");
    assert!(!text.contains("trgLang"), "{text}");
    assert!(!text.contains("<target"), "{text}");
    assert!(
        text.contains("<source xml:space=\"preserve\">Goodbye.</source>"),
        "{text}"
    );
}

#[test]
fn export_marks_what_a_person_accepted_and_what_is_stale() {
    let (_dir, root) = project("exchange-export-state");
    assert_eq!(plain(&root, &["translations", "update"]).0, 0);
    assert_eq!(plain(&root, &["translations", "accept", "de_de"]).0, 0);
    write(
        &root,
        "domains/shipping/translations/en_gb.jadpo",
        &FILES[5].1.replace("Goodbye.", "Bye."),
    );
    assert_eq!(plain(&root, &["translations", "export", "de_de"]).0, 0);
    let text = read(&root, "build/translations/de_de.xlf");
    // Accepted wording is `reviewed`; the stale one says it is.
    assert!(text.contains("<segment state=\"reviewed\">"), "{text}");
    assert!(
        text.contains(
            "<note category=\"stale\">The default wording changed after this was translated.</note>"
        ),
        "{text}"
    );
    assert!(
        text.contains("<source xml:space=\"preserve\">Bye.</source>"),
        "{text}"
    );
}

/// The shipping project with a route, so Jadpo's request messages are on,
/// and Polish but no German.
fn with_routes(root: &Path) {
    write(
        root,
        "app.jadpo",
        "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.pl_pl]\n}\n",
    );
    std::fs::remove_file(root.join(DE_DE)).expect("remove");
    write(
        root,
        "policy.jadpo",
        "use planning\n\npolicy {\n    actions: {\n        planning.find: [Access.public]\n    }\n}\n",
    );
    write(
        root,
        "domains/planning/types/todo_ref.jadpo",
        "/// A todo's reference.\ntype TodoRef = Text {\n    max_length: 40\n}\n",
    );
    write(
        root,
        "domains/planning/types/found.jadpo",
        "use planning/types { TodoRef }\n\n/// A todo found.\ntype Found = Output {\n    reference: TodoRef\n}\n",
    );
    write(
        root,
        "domains/planning/types/find_request.jadpo",
        "use planning/types { TodoRef }\n\n/// A todo to find.\ntype FindRequest = Input {\n    reference: TodoRef\n}\n",
    );
    write(
        root,
        "domains/planning/actions/find.jadpo",
        "use planning/types { Found, TodoRef }\n\n/// Finds a todo.\naction find(reference: TodoRef) -> Found {\n    return Found {\n        reference: reference\n    }\n}\n",
    );
    write(
        root,
        "domains/planning/routes/todos.jadpo",
        "use planning/types { FindRequest, Found }\nuse planning/actions { find }\n\nroutes Todos {\n    prefix: /todos\n\n    POST / {\n        authentication: RouteAuthentication.none\n        input: FindRequest\n        output: Found\n        run: find(input.reference)\n    }\n}\n",
    );
}

#[test]
fn export_includes_jadpos_own_messages() {
    let (_dir, root) = project("exchange-export-jadpo");
    with_routes(&root);
    let (code, out) = plain(
        &root,
        &["translations", "export", "pl_pl", "--format", "json"],
    );
    assert_eq!(code, 0, "{out}");
    let document: serde_json::Value =
        serde_json::from_str(&read(&root, "build/translations/pl_pl.json")).expect("json");
    assert_eq!(document["jadpo.invalid_input"]["message"], "", "{document}");
    assert!(
        document["jadpo.invalid_input"]["description"]
            .as_str()
            .is_some_and(|text| !text.is_empty()),
        "{document}"
    );
    // `TextDirection`'s wording is the same in every language: not sent.
    assert!(document.get("jadpo.left_to_right").is_none(), "{document}");
    assert!(
        document.get("jadpo.text_direction.left_to_right").is_none(),
        "{document}"
    );
    let source = plain(
        &root,
        &["translations", "export", "en_gb", "--format", "json"],
    );
    assert_eq!(source.0, 0, "{}", source.1);
    let source: serde_json::Value =
        serde_json::from_str(&read(&root, "build/translations/en_gb.json")).expect("json");
    assert_eq!(
        source["jadpo.invalid_input"]["message"],
        "Some of the information sent isn't valid. Check it and try again."
    );
}

#[test]
fn export_as_data() {
    let (_dir, root) = project("exchange-export-data");
    let done = jadpo(&root, &["translations", "export", "--json"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    let document: serde_json::Value = serde_json::from_str(&done.stdout).expect("json");
    let files = document["data"]["files"].as_array().expect("files");
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["locale"], "de_de");
    assert_eq!(files[0]["path"], "build/translations/de_de.xlf");
    assert_eq!(files[0]["format"], "xliff");
    assert_eq!(files[0]["units"], 5);
    assert_eq!(files[0]["translated"], 5);
    assert_eq!(files[1]["translated"], 0);
    assert_eq!(files[1]["units"], 7);
}

#[test]
fn export_refuses_what_it_cannot_do() {
    let (_dir, root) = project("exchange-export-refuse");
    for (args, needle) in [
        (
            vec!["translations", "export", "fr_fr"],
            "`fr_fr` isn't a locale with translations",
        ),
        (
            vec!["translations", "export", "de_d"],
            "Did you mean `de_de`?",
        ),
        (
            vec!["translations", "export", "de_de", "pl_pl"],
            "takes 2 arguments",
        ),
        (
            vec!["translations", "export", "--format", "csv"],
            "`csv` isn't a format",
        ),
    ] {
        let (code, out) = plain(&root, &args);
        assert_eq!(code, 2, "{args:?}: {out}");
        assert!(out.contains("rule: cli.translations_arguments"), "{out}");
        assert!(out.contains(needle), "{args:?}: {out}");
    }
    assert!(!root.join("build/translations").exists());
}

// --- import --------------------------------------------------------------------------

#[test]
fn exporting_and_importing_unchanged_changes_nothing() {
    for format in ["xliff", "json"] {
        let (_dir, root) = project(&format!("exchange-round-trip-{format}"));
        assert_eq!(plain(&root, &["translations", "update"]).0, 0);
        assert_eq!(plain(&root, &["translations", "accept", "de_de"]).0, 0);
        let exported = plain(
            &root,
            &["translations", "export", "de_de", "--format", format],
        );
        assert_eq!(exported.0, 0, "{}", exported.1);
        let before = snapshot(&root);
        let file = format!(
            "build/translations/de_de.{}",
            if format == "xliff" { "xlf" } else { "json" }
        );
        let (code, out) = plain(&root, &["translations", "import", &file]);
        assert_eq!(code, 0, "{format}: {out}");
        assert_eq!(out, "Already up to date: de_de, 5 unchanged\n", "{format}");
        assert_eq!(snapshot(&root), before, "{format}");
    }
}

#[test]
fn a_round_trip_keeps_a_stale_translation_stale() {
    let (_dir, root) = project("exchange-round-trip-stale");
    assert_eq!(plain(&root, &["translations", "update"]).0, 0);
    write(
        &root,
        "domains/shipping/translations/en_gb.jadpo",
        &FILES[5].1.replace("Goodbye.", "Bye."),
    );
    assert_eq!(plain(&root, &["translations", "export", "de_de"]).0, 0);
    let before = snapshot(&root);
    let (code, out) = plain(
        &root,
        &["translations", "import", "build/translations/de_de.xlf"],
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(snapshot(&root), before);
}

#[test]
fn import_creates_a_language_file_and_records_it() {
    let (_dir, root) = project("exchange-import-new");
    write(&root, "from-service/pl_pl.xlf", POLISH);
    let (code, out) = plain(&root, &["translations", "import", "from-service/pl_pl.xlf"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out,
        "Wrote domains/shipping/translations/pl_pl.jadpo
    farewell              added
    order_shipped.formal  added
    order_shipped.casual  added
    parcels.one           added
    parcels.few           added
    parcels.many          added
    parcels.other         added

Wrote domains/shipping/translations/sources.jadpo_record

Imported pl_pl: 7 added

next: jadpo check  confirms the imported wording
"
    );
    assert_eq!(
        read(&root, PL_PL),
        "translations {
    farewell: \"Do widzenia.\"
    order_shipped: {
        formal: \"Dzień dobry {name}, Twoje zamówienie zostało wysłane.\"
        casual: \"Cześć {name}, Twoja paczka jest w drodze!\"
    }
    parcels: {
        one: \"{count} paczka\"
        few: \"{count} paczki\"
        many: \"{count} paczek\"
        other: \"{count} paczki\"
    }
}
"
    );
    let record = read(&root, RECORD);
    for member in ["pl_pl.farewell", "pl_pl.order_shipped", "pl_pl.parcels"] {
        assert!(
            record.contains(&format!("translation {member} from ")),
            "{record}"
        );
    }
    assert!(!record.contains("accepted"), "{record}");
    // The result checks and is formatted.
    let (code, check) = plain(&root, &["check"]);
    assert_eq!(code, 0, "{check}");
    let (code, fmt) = plain(&root, &["fmt", "--check"]);
    assert_eq!(code, 0, "{fmt}");
    // And nothing is stale, so importing it again changes nothing.
    let again = plain(&root, &["translations", "import", "from-service/pl_pl.xlf"]);
    assert_eq!(again.1, "Already up to date: pl_pl, 7 unchanged\n");
}

#[test]
fn import_rewords_a_member_in_place_and_drops_its_acceptance() {
    let (_dir, root) = project("exchange-import-reword");
    write(
        &root,
        "app.jadpo",
        "application {\n    translations: {\n        review: TranslationReview.required\n    }\n}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.de_de, Locale.pl_pl]\n}\n",
    );
    assert_eq!(plain(&root, &["translations", "update"]).0, 0);
    assert_eq!(plain(&root, &["translations", "accept", "de_de"]).0, 0);
    let text = r#"<xliff xmlns="urn:oasis:names:tc:xliff:document:2.0" version="2.0" srcLang="en-GB" trgLang="de-DE">
  <file id="shipping">
    <unit id="farewell"><segment><source>Goodbye.</source><target>Bis bald.</target></segment></unit>
    <unit id="parcels.other"><segment><source>{count} parcels</source><target>{count} Pakete</target></segment></unit>
    <unit id="parcels.one"><segment><source>{count} parcel</source><target>{count} Päckchen</target></segment></unit>
  </file>
</xliff>
"#;
    write(&root, "de.xlf", text);
    let (code, out) = plain(&root, &["translations", "import", "de.xlf"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out,
        "Wrote domains/shipping/translations/de_de.jadpo
    farewell     reworded
    parcels.one  reworded

Wrote domains/shipping/translations/sources.jadpo_record

Imported de_de: 2 reworded, 1 unchanged

next: jadpo check  confirms the imported wording
"
    );
    let written = read(&root, DE_DE);
    // The comment, the other member and the order stay as they were.
    assert!(written.contains("// Reviewed by Jana."), "{written}");
    assert!(
        written.contains("Guten Tag {name}, Ihre Bestellung wurde versandt."),
        "{written}"
    );
    assert!(written.contains("farewell: \"Bis bald.\""), "{written}");
    assert!(written.contains("one: \"{count} Päckchen\""), "{written}");
    assert!(written.contains("other: \"{count} Pakete\""), "{written}");
    // What was reworded is no longer accepted; the rest still is.
    let (_, left) = plain(&root, &["translations", "missing", "de_de"]);
    assert!(left.contains("farewell"), "{left}");
    assert!(left.contains("parcels"), "{left}");
    assert!(!left.contains("order_shipped"), "{left}");
}

#[test]
fn import_reads_json_with_the_locale_from_the_file_name() {
    let (_dir, root) = project("exchange-import-json");
    write(
        &root,
        "from-service/pl_pl.json",
        r#"{
  "shipping.farewell": { "message": "Do widzenia.", "description": "ignored" },
  "shipping.order_shipped.formal": "Dzień dobry {name}.",
  "shipping.order_shipped.casual": "Cześć {name}!",
  "shipping.parcels.one": { "message": "{count} paczka" },
  "shipping.parcels.few": { "message": "{count} paczki" },
  "shipping.parcels.many": { "message": "{count} paczek" },
  "shipping.parcels.other": { "message": "{count} paczki" }
}
"#,
    );
    let (code, out) = plain(
        &root,
        &["translations", "import", "from-service/pl_pl.json"],
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("Imported pl_pl: 7 added"), "{out}");
    assert_eq!(plain(&root, &["check"]).0, 0);
    // A name that is no locale needs it said.
    write(
        &root,
        "export.json",
        &read(&root, "from-service/pl_pl.json"),
    );
    let (code, out) = plain(&root, &["translations", "import", "export.json"]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("rule: cli.translations_arguments"), "{out}");
    assert!(out.contains("`--locale`"), "{out}");
    let (code, out) = plain(
        &root,
        &["translations", "import", "export.json", "--locale", "pl_pl"],
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.starts_with("Already up to date: pl_pl"), "{out}");
}

#[test]
fn untranslated_units_are_left_alone() {
    let (_dir, root) = project("exchange-import-partial");
    assert_eq!(plain(&root, &["translations", "export", "pl_pl"]).0, 0);
    let (code, out) = plain(
        &root,
        &["translations", "import", "build/translations/pl_pl.xlf"],
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(out, "Already up to date: pl_pl, 7 not translated yet\n");
    assert!(!root.join(PL_PL).exists());
}

#[test]
fn import_refuses_wording_the_language_files_would_refuse() {
    // Each is refused with the rule the language file would be given, at
    // the unit in the file that was imported, and nothing is written.
    let cases: [(&str, &str, &[&str]); 3] = [
        (
            "a placeholder the message does not have",
            "<target>Do widzenia, {name}.</target>",
            &[
                "rule: translation.placeholder_unknown",
                "`{name}` isn't a field of `Farewell`",
            ],
        ),
        (
            "a placeholder with its braces broken",
            "<target>Do widzenia {.</target>",
            &["rule: translation.wording_invalid"],
        ),
        (
            "a misspelt placeholder",
            "<target>Do widzenia {nmae}</target>",
            &["rule: translation.placeholder_unknown"],
        ),
    ];
    for (name, target, expected) in cases {
        let (_dir, root) = project("exchange-import-refuse");
        let unit = format!(
            "<xliff xmlns=\"urn:oasis:names:tc:xliff:document:2.0\" version=\"2.0\" srcLang=\"en-GB\" trgLang=\"pl-PL\">\n  <file id=\"shipping\">\n    <unit id=\"farewell\"><segment><source>Goodbye.</source>{target}</segment></unit>\n  </file>\n</xliff>\n"
        );
        write(&root, "pl.xlf", &unit);
        let before = snapshot(&root);
        let (code, out) = plain(&root, &["translations", "import", "pl.xlf"]);
        assert_eq!(code, 1, "{name}: {out}");
        for needle in expected {
            assert!(out.contains(needle), "{name}: missing `{needle}`:\n{out}");
        }
        assert!(out.contains("--> pl.xlf"), "{name}: {out}");
        assert!(out.contains("nothing written"), "{name}: {out}");
        assert_eq!(snapshot(&root), before, "{name}");
    }
}

#[test]
fn import_refuses_dropping_a_placeholder_from_every_wording() {
    let (_dir, root) = project("exchange-import-unused");
    let text = POLISH
        .replace(
            "Dzień dobry {name}, Twoje zamówienie zostało wysłane.",
            "Dzień dobry, Twoje zamówienie zostało wysłane.",
        )
        .replace(
            "Cześć {name}, Twoja paczka jest w drodze!",
            "Cześć, Twoja paczka jest w drodze!",
        );
    write(&root, "pl.xlf", &text);
    let before = snapshot(&root);
    let (code, out) = plain(&root, &["translations", "import", "pl.xlf"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("rule: translation.field_unused"), "{out}");
    assert!(out.contains("--> pl.xlf"), "{out}");
    assert_eq!(snapshot(&root), before);
}

#[test]
fn import_refuses_a_plural_form_the_language_lacks_and_one_it_needs() {
    let (_dir, root) = project("exchange-import-forms");
    // Polish without `many`, and `two`, which Polish has no use for.
    let text = POLISH
        .replace(
            "    <unit id=\"parcels.many\"><segment><source>{count} parcels</source><target>{count} paczek</target></segment></unit>\n",
            "    <unit id=\"parcels.two\"><segment><source>{count} parcels</source><target>{count} paczki</target></segment></unit>\n",
        );
    write(&root, "pl.xlf", &text);
    let before = snapshot(&root);
    let (code, out) = plain(&root, &["translations", "import", "pl.xlf"]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("rule: translation.exchange_unit_unknown"),
        "{out}"
    );
    assert!(out.contains("`shipping.parcels.two`"), "{out}");
    assert!(out.contains("`one`, `few`, `many` and `other`"), "{out}");
    assert_eq!(snapshot(&root), before);

    // Without `two`, `many` is simply missing: the language file would be
    // refused for it, so the import is.
    let text = POLISH.replace(
        "    <unit id=\"parcels.many\"><segment><source>{count} parcels</source><target>{count} paczek</target></segment></unit>\n",
        "",
    );
    write(&root, "pl.xlf", &text);
    let before = snapshot(&root);
    let (code, out) = plain(&root, &["translations", "import", "pl.xlf"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("rule: translation.variant_missing"), "{out}");
    assert!(out.contains("`many`"), "{out}");
    assert!(out.contains("--> pl.xlf"), "{out}");
    assert_eq!(snapshot(&root), before);
}

#[test]
fn import_refuses_an_enum_value_that_does_not_exist_or_is_missing() {
    let (_dir, root) = project("exchange-import-values");
    let text = POLISH.replace("order_shipped.casual", "order_shipped.casul");
    write(&root, "pl.xlf", &text);
    let before = snapshot(&root);
    let (code, out) = plain(&root, &["translations", "import", "pl.xlf"]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("rule: translation.exchange_unit_unknown"),
        "{out}"
    );
    assert!(out.contains("`shipping.order_shipped.casul`"), "{out}");
    assert!(
        out.contains("Did you mean `shipping.order_shipped.casual`?"),
        "{out}"
    );
    assert_eq!(snapshot(&root), before);

    let text = POLISH.replace(
        "    <unit id=\"order_shipped.casual\"><segment><source>Hi {name}, your order is on its way!</source><target>Cześć {name}, Twoja paczka jest w drodze!</target></segment></unit>\n",
        "",
    );
    write(&root, "pl.xlf", &text);
    let before = snapshot(&root);
    let (code, out) = plain(&root, &["translations", "import", "pl.xlf"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("rule: translation.variant_missing"), "{out}");
    assert!(out.contains("`casual`"), "{out}");
    assert_eq!(snapshot(&root), before);
}

#[test]
fn import_leaves_an_in_progress_locale_partly_translated() {
    let (_dir, root) = project("exchange-import-in-progress");
    write(
        &root,
        "app.jadpo",
        "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.de_de, Locale.pl_pl]\n    in_progress: [Locale.pl_pl]\n}\n",
    );
    let text = POLISH.replace(
        "    <unit id=\"parcels.many\"><segment><source>{count} parcels</source><target>{count} paczek</target></segment></unit>\n",
        "",
    );
    write(&root, "pl.xlf", &text);
    let (code, out) = plain(&root, &["translations", "import", "pl.xlf"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(plain(&root, &["check"]).0, 0);
}

#[test]
fn import_refuses_a_file_it_cannot_read() {
    let (_dir, root) = project("exchange-import-unreadable");
    let two = |body: &str| {
        format!(
            "<xliff xmlns=\"urn:oasis:names:tc:xliff:document:2.0\" version=\"2.0\" srcLang=\"en-GB\" trgLang=\"pl-PL\">{body}</xliff>"
        )
    };
    let before = snapshot(&root);
    let cases: Vec<(String, &str)> = vec![
        ("not xml at all".to_string(), "isn't a translation file"),
        ("<xliff".to_string(), "not well-formed XML"),
        (
            "<xliff xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" version=\"1.2\"/>".to_string(),
            "XLIFF 1.2",
        ),
        (
            two("<file id=\"shipping\"><unit id=\"farewell\"><segment/><segment/></unit></file>"),
            "2 segments",
        ),
        (
            two(
                "<file id=\"shipping\"><unit id=\"farewell\"><segment/></unit><unit id=\"farewell\"><segment/></unit></file>",
            ),
            "twice",
        ),
        (
            two("<file id=\"shipping\"></file>").replace("srcLang=\"en-GB\"", "srcLang=\"fr-FR\""),
            "translated from `fr-FR`",
        ),
        ("{\"a\": 3}".to_string(), "neither a string nor an object"),
        ("[1]".to_string(), "not an object of messages"),
    ];
    for (text, needle) in cases {
        write(&root, "bad.xlf", &text);
        let (code, out) = plain(
            &root,
            &["translations", "import", "bad.xlf", "--locale", "pl_pl"],
        );
        assert_eq!(code, 1, "{text}: {out}");
        assert!(
            out.contains("rule: translation.exchange_file_invalid"),
            "{text}: {out}"
        );
        assert!(out.contains(needle), "{text}: missing `{needle}`:\n{out}");
        assert_eq!(snapshot(&root).len(), before.len() + 1);
    }
    let (code, out) = plain(&root, &["translations", "import", "missing.xlf"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("rule: toolchain.source_read_failed"), "{out}");
}

#[test]
fn import_refuses_the_wrong_locale() {
    let (_dir, root) = project("exchange-import-locale");
    write(&root, "pl.xlf", POLISH);
    for (args, needle) in [
        (
            vec!["translations", "import", "pl.xlf", "--locale", "de_de"],
            "says it is `pl-PL`",
        ),
        (
            vec!["translations", "import", "pl.xlf", "--locale", "en_gb"],
            "isn't a locale with translations",
        ),
        (
            vec!["translations", "import", "pl.xlf", "--locale", "fr_fr"],
            "isn't a locale with translations",
        ),
        (vec!["translations", "import"], "needs a file"),
    ] {
        let (code, out) = plain(&root, &args);
        assert_eq!(code, 2, "{args:?}: {out}");
        assert!(
            out.contains("rule: cli.translations_arguments"),
            "{args:?}: {out}"
        );
        assert!(out.contains(needle), "{args:?}: {out}");
    }
}

#[test]
fn import_refuses_a_unit_that_is_not_in_the_project() {
    let (_dir, root) = project("exchange-import-unknown");
    let text = POLISH.replace("id=\"farewell\"", "id=\"farewel\"");
    write(&root, "pl.xlf", &text);
    let before = snapshot(&root);
    let (code, out) = plain(&root, &["translations", "import", "pl.xlf"]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("rule: translation.exchange_unit_unknown"),
        "{out}"
    );
    assert!(out.contains("`shipping.farewel`"), "{out}");
    assert!(out.contains("Did you mean `shipping.farewell`?"), "{out}");
    assert!(out.contains("--> pl.xlf"), "{out}");
    assert_eq!(snapshot(&root), before);
}

#[test]
fn a_translation_of_wording_that_has_changed_is_written_and_stays_stale() {
    let (_dir, root) = project("exchange-import-stale");
    write(
        &root,
        "domains/shipping/translations/en_gb.jadpo",
        &FILES[5].1.replace("Goodbye.", "Bye."),
    );
    // The file the service returns was made from "Goodbye.".
    write(&root, "pl.xlf", POLISH);
    let (code, out) = plain(&root, &["translations", "import", "pl.xlf"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("Imported pl_pl: 7 added, 1 stale"), "{out}");
    assert!(out.contains("next: jadpo translations missing"), "{out}");
    let (code, check) = plain(&root, &["check"]);
    assert_eq!(code, 1, "{check}");
    assert_eq!(
        check.matches("rule: translation.stale").count(),
        1,
        "{check}"
    );
    assert!(check.contains("`farewell` in `pl_pl`"), "{check}");
}

#[test]
fn import_refuses_a_record_it_cannot_read() {
    let (_dir, root) = project("exchange-import-record");
    write(&root, RECORD, "garbage\n");
    write(&root, "pl.xlf", POLISH);
    let before = snapshot(&root);
    let (code, out) = plain(&root, &["translations", "import", "pl.xlf"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("rule: record.edited"), "{out}");
    assert_eq!(snapshot(&root), before);
}

#[test]
fn import_writes_jadpos_own_messages_to_the_root_file() {
    let (_dir, root) = project("exchange-import-jadpo");
    with_routes(&root);
    assert_eq!(
        plain(
            &root,
            &["translations", "export", "pl_pl", "--format", "json"]
        )
        .0,
        0
    );
    let exported: serde_json::Value =
        serde_json::from_str(&read(&root, "build/translations/pl_pl.json")).expect("json");
    let mut filled = serde_json::Map::new();
    for (id, value) in exported.as_object().expect("object") {
        let message = if id.starts_with("jadpo.") {
            format!("Polski: {}", id.trim_start_matches("jadpo."))
        } else {
            value["message"].as_str().unwrap_or_default().to_string()
        };
        filled.insert(id.clone(), serde_json::json!({ "message": message }));
    }
    // The shipping messages come in the usual way.
    for (id, message) in [
        ("shipping.farewell", "Do widzenia."),
        ("shipping.order_shipped.formal", "Dzień dobry {name}."),
        ("shipping.order_shipped.casual", "Cześć {name}."),
        ("shipping.parcels.one", "{count} paczka"),
        ("shipping.parcels.few", "{count} paczki"),
        ("shipping.parcels.many", "{count} paczek"),
        ("shipping.parcels.other", "{count} paczki"),
    ] {
        filled.insert(id.to_string(), serde_json::json!({ "message": message }));
    }
    write(
        &root,
        "pl_pl.json",
        &serde_json::Value::Object(filled).to_string(),
    );
    let (code, out) = plain(&root, &["translations", "import", "pl_pl.json"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("Wrote translations/pl_pl.jadpo"), "{out}");
    assert!(
        out.contains("Wrote translations/sources.jadpo_record"),
        "{out}"
    );
    let root_file = read(&root, "translations/pl_pl.jadpo");
    assert!(
        root_file.starts_with("translations {\n    jadpo: {\n"),
        "{root_file}"
    );
    assert!(
        root_file.contains("invalid_input: \"Polski: invalid_input\""),
        "{root_file}"
    );
    assert!(
        read(&root, "translations/sources.jadpo_record")
            .contains("translation pl_pl.jadpo.invalid_input from ")
    );
    assert_eq!(plain(&root, &["records", "update"]).0, 0);
    let (code, check) = plain(&root, &["check"]);
    assert_eq!(code, 0, "{check}");
}
