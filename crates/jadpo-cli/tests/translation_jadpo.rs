//! Jadpo's own text from the command line (area 10, "Jadpo's and
//! packages' text" and "Tooling"; chunk 5.4e): `jadpo translations
//! builtin`, Jadpo's failures in the per-locale export for client apps, the
//! root file's wording in `missing`, and its record.
//!
//! The projects have no entities, so no database is needed.

use super::http::{jadpo, tempdir, write};

const POLICY: &str =
    "use planning\n\npolicy {\n    actions: {\n        planning.find: [Access.public]\n    }\n}\n";

const FILES: &[(&str, &str)] = &[
    ("policy.jadpo", POLICY),
    (
        "domains/planning/types/todo_ref.jadpo",
        "/// A todo's reference.\ntype TodoRef = Text {\n    max_length: 40\n}\n",
    ),
    (
        "domains/planning/types/found.jadpo",
        "use planning/types { TodoRef }\n\n/// A todo found.\ntype Found = Output {\n    reference: TodoRef\n}\n",
    ),
    (
        "domains/planning/types/find_request.jadpo",
        "use planning/types { TodoRef }\n\n/// A todo to find.\ntype FindRequest = Input {\n    reference: TodoRef\n}\n",
    ),
    (
        "domains/planning/actions/find.jadpo",
        "use planning/types { Found, TodoRef }\n\n/// Finds a todo.\naction find(reference: TodoRef) -> Found {\n    return Found {\n        reference: reference\n    }\n}\n",
    ),
    (
        "domains/planning/routes/todos.jadpo",
        "use planning/types { FindRequest, Found }\nuse planning/actions { find }\n\nroutes Todos {\n    prefix: /todos\n\n    POST / {\n        authentication: RouteAuthentication.none\n        input: FindRequest\n        output: Found\n        run: find(input.reference)\n    }\n}\n",
    ),
];

const REQUESTS: [&str; 10] = [
    "deadline_exceeded",
    "idempotency_key_reused",
    "internal_fault",
    "invalid_input",
    "invalid_request",
    "method_not_allowed",
    "rate_limited",
    "request_too_large",
    "route_not_found",
    "unavailable",
];

fn app(supported: &str, in_progress: &str) -> String {
    format!(
        "application {{}}\n\nlocales {{\n    default: Locale.en_gb\n    supported: [{supported}]\n{in_progress}}}\n"
    )
}

fn words(members: &[&str]) -> String {
    let mut out = String::from("translations {\n    jadpo: {\n");
    for member in members {
        out.push_str(&format!("        {member}: \"Texte de {member}.\"\n"));
    }
    out.push_str("    }\n}\n");
    out
}

fn planning(name: &str, supported: &str, in_progress: &str) -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new(name);
    let root = dir.path().join("planning");
    write(&root, "app.jadpo", &app(supported, in_progress));
    for (path, text) in FILES {
        write(&root, path, text);
    }
    (dir, root)
}

fn plain(root: &std::path::Path, args: &[&str]) -> (u8, String) {
    let done = jadpo(root, args);
    (done.code, format!("{}{}", done.stdout, done.stderr))
}

fn checked(root: &std::path::Path) -> (u8, String) {
    assert_eq!(plain(root, &["api", "update"]).0, 0);
    plain(root, &["check"])
}

#[test]
fn builtin_lists_the_messages_of_the_features_the_app_has_on() {
    let (_dir, root) = planning("jadpo-builtin", "Locale.en_gb, Locale.en_us", "");
    let (code, out) = plain(&root, &["translations", "builtin"]);
    assert_eq!(code, 0, "{out}");
    // The requests group is on, with its wording to translate from.
    assert!(out.starts_with("Requests\n"), "{out}");
    assert!(out.contains("\n  invalid_input"), "{out}");
    assert!(
        out.contains("    Some of the information sent isn't valid. Check it and try again.\n"),
        "{out}"
    );
    // Sign-in and magic-link messages belong to features this app has off.
    assert!(!out.contains("authentication_required"), "{out}");
    assert!(!out.contains("magic_link_email"), "{out}");
    // Both locales are shipped, so nothing is left to word.
    assert!(out.contains("11 messages"), "{out}");
    assert!(!out.contains("needs"), "{out}");
}

#[test]
fn builtin_says_which_locales_the_app_still_has_to_word() {
    let (_dir, root) = planning("jadpo-builtin-needs", "Locale.en_gb, Locale.fr_fr", "");
    let (code, out) = plain(&root, &["translations", "builtin"]);
    assert_eq!(code, 0, "{out}");
    let line = out
        .lines()
        .find(|line| line.starts_with("  invalid_input "))
        .unwrap_or_default();
    assert!(line.ends_with("  needs fr_fr"), "{out}");
    assert!(out.contains("11 messages: 10 need wording"), "{out}");
    // The shipped wording is replaced by the app's.
    write(&root, "translations/fr_fr.jadpo", &words(&REQUESTS));
    let (_, now) = plain(&root, &["translations", "builtin"]);
    assert!(!now.contains("needs"), "{now}");
}

#[test]
fn builtin_as_data() {
    let (_dir, root) = planning("jadpo-builtin-json", "Locale.en_gb, Locale.fr_fr", "");
    let done = jadpo(&root, &["translations", "builtin", "--json"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    let document: serde_json::Value = serde_json::from_str(&done.stdout).expect("json");
    let messages = document["data"]["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 11);
    let invalid_input = messages
        .iter()
        .find(|message| message["key"] == "invalid_input")
        .expect("invalid_input");
    assert_eq!(invalid_input["feature"], "requests");
    assert_eq!(invalid_input["kind"], "failure");
    assert_eq!(
        invalid_input["wording"],
        "Some of the information sent isn't valid. Check it and try again."
    );
    assert_eq!(
        invalid_input["shippedIn"],
        serde_json::json!(["en_gb", "en_us"])
    );
    assert_eq!(invalid_input["needs"], serde_json::json!(["fr_fr"]));
    assert!(
        invalid_input["fields"]
            .as_array()
            .expect("fields")
            .is_empty()
    );
}

#[test]
fn builtin_has_nothing_to_list_without_locales() {
    let dir = tempdir::Dir::new("jadpo-builtin-none");
    let root = dir.path().join("plain");
    write(&root, "app.jadpo", "application {}\n");
    assert_eq!(
        plain(&root, &["translations", "builtin"]),
        (0, "No messages\n".to_string())
    );
}

#[test]
fn jadpo_failures_join_each_apis_export_for_client_apps() {
    let (_dir, root) = planning("jadpo-export", "Locale.en_gb, Locale.fr_fr", "");
    let mut fr = words(&REQUESTS);
    fr = fr.replace(
        "invalid_input: \"Texte de invalid_input.\"",
        "invalid_input: \"Des informations ne sont pas valides.\"",
    );
    write(&root, "translations/fr_fr.jadpo", &fr);
    let (code, out) = checked(&root);
    assert_eq!(code, 0, "{out}");
    let read = |locale: &str| {
        let text = std::fs::read_to_string(
            root.join("build/openapi")
                .join(format!("main.failures.{locale}.json")),
        )
        .unwrap_or_else(|error| panic!("{locale}: {error}"));
        serde_json::from_str::<serde_json::Value>(&text).expect("json")
    };
    let en = read("en_gb");
    let codes: Vec<&str> = en
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(codes, REQUESTS, "{en}");
    assert_eq!(
        en["invalid_input"],
        serde_json::json!({
            "formats": {},
            "text": "Some of the information sent isn't valid. Check it and try again."
        })
    );
    assert_eq!(
        read("fr_fr")["invalid_input"]["text"],
        "Des informations ne sont pas valides."
    );
}

#[test]
fn an_app_rewords_what_jadpo_ships_and_the_export_follows() {
    let (_dir, root) = planning("jadpo-reword", "Locale.en_gb, Locale.en_us", "");
    write(
        &root,
        "translations/en_gb.jadpo",
        "translations {\n    jadpo: {\n        rate_limited: \"Steady on: {app_name} is busy.\"\n    }\n}\n",
    );
    let (code, out) = checked(&root);
    assert_eq!(code, 0, "{out}");
    let text = std::fs::read_to_string(root.join("build/openapi/main.failures.en_gb.json"))
        .expect("export");
    let document: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(
        document["rate_limited"]["text"],
        "Steady on: {app_name} is busy."
    );
    // The other locale keeps what Jadpo ships.
    let us = std::fs::read_to_string(root.join("build/openapi/main.failures.en_us.json"))
        .expect("export");
    assert!(us.contains("Too many requests. Wait a moment"), "{us}");
}

#[test]
fn a_locale_in_progress_is_listed_and_falls_back_to_what_jadpo_ships() {
    let (_dir, root) = planning(
        "jadpo-missing",
        "Locale.en_gb, Locale.fr_fr",
        "    in_progress: [Locale.fr_fr]\n",
    );
    let (code, out) = checked(&root);
    assert_eq!(code, 0, "{out}");
    let (_, listed) = plain(&root, &["translations", "missing"]);
    assert!(
        listed.starts_with("translations/fr_fr.jadpo  no file yet\n"),
        "{listed}"
    );
    let line = listed
        .lines()
        .find(|line| line.starts_with("  jadpo.invalid_input "))
        .unwrap_or_default();
    assert!(
        line.ends_with("missing     falls back to en_gb"),
        "{listed}"
    );
    assert!(
        listed.contains("10 outstanding: 10 missing, 1 locale"),
        "{listed}"
    );
    // The export has text for it all the same.
    let text = std::fs::read_to_string(root.join("build/openapi/main.failures.fr_fr.json"))
        .expect("export");
    assert!(
        text.contains("Some of the information sent isn't valid"),
        "{text}"
    );
}

#[test]
fn a_locale_that_is_not_in_progress_must_word_jadpo_before_the_check_passes() {
    let (_dir, root) = planning("jadpo-check", "Locale.en_gb, Locale.fr_fr", "");
    let (code, out) = checked(&root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("rule: translation.jadpo_missing"), "{out}");
    assert!(
        out.contains("Run `jadpo translations builtin`"),
        "the error points at the command that lists them:\n{out}"
    );
    write(&root, "translations/fr_fr.jadpo", &words(&REQUESTS));
    let (code, out) = checked(&root);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn update_records_the_apps_wording_of_jadpo_in_the_root_folder() {
    let (_dir, root) = planning("jadpo-record", "Locale.en_gb, Locale.fr_fr", "");
    write(
        &root,
        "translations/fr_fr.jadpo",
        &words(&["invalid_input"]),
    );
    let (code, out) = plain(&root, &["translations", "update"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.starts_with("Wrote translations/sources.jadpo_record\n"),
        "{out}"
    );
    assert!(out.contains("fr_fr.jadpo.invalid_input  recorded"), "{out}");
    let record =
        std::fs::read_to_string(root.join("translations/sources.jadpo_record")).expect("record");
    assert!(
        record.contains("\ntranslation fr_fr.jadpo.invalid_input from "),
        "{record}"
    );
    // Shipped English changes by a release, not here, so it is the app's
    // own default wording that a translation is made from.
    write(
        &root,
        "translations/en_gb.jadpo",
        "translations {\n    jadpo: {\n        invalid_input: \"Check the form.\"\n    }\n}\n",
    );
    let (code, out) = checked(&root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("rule: translation.stale"), "{out}");
}
