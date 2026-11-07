//! A provider package's types, checked by both the compiler's names and an
//! agent's review before it is published (area 11, "Shape and name
//! detection" and "The scoped review and unconfirmed operations"): text
//! named like personal data is `package.standard_candidate`, a package
//! written by hand lists its reviewed types in `provider { }`, `jadpo
//! check` lists those its calls reach still to review, and the publishing
//! bar's `reviewed` row fails a package with any public type unreviewed.

use std::path::{Path, PathBuf};

use super::http::{tempdir, write};
use super::packages::plain;

/// A mail API written by hand, as Resend's package was first released:
/// its request's addresses and words all plain text.
const REQUEST: &str = "/// One email to send.
public type SendEmailRequest = Object {
    from: Text
    to: Text
    subject: Text
    html: Text
}
";

const RESPONSE: &str = "/// What the API answers an email it took with.
public type SendEmailResponse = Object {
    id: Text
}
";

const PROVIDER: &str = "/// A mail API, each request carrying the account's API key.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: \"https://api.mailer.example\"
        sandbox: \"https://api.mailer.example\"
    }
    credential: CredentialScheme.bearer
}
";

const EMAILS: &str = "public resource emails {
    send: {
        method: HttpMethod.post
        path: \"/emails\"
        input: SendEmailRequest
        output: SendEmailResponse
        changes: true
        source: PackageSource.written
        docs: \"https://mailer.example/docs/send\"
    }
}
";

/// The request with its addresses swapped for `Email` and its words
/// marked as the personal data they may be.
const FIXED: &str = "/// One email to send.
public type SendEmailRequest = Object {
    from: Email
    to: List<Email>
    subject: Text {
        personal: PersonalData.identifying
    }
    html: Text {
        personal: PersonalData.identifying
    }
}
";

/// An app with the mail API as its own integration, calling it once.
fn app(dir: &tempdir::Dir) -> PathBuf {
    let root = dir.path().join("desk");
    let folder = "shared/providers/mailer/v1";
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        mail: {\n            package: Mailer.v1\n            domains: [notices]\n            credential: config.mail_key\n            environment: config.mail_environment\n        }\n    }\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    mail_key: Text {\n        secret: true\n    }\n    mail_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n}\n",
    );
    write(
        &root,
        &format!("{folder}/package.jadpo"),
        "/// A mail API, written by hand from its documentation.\npackage Mailer.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    );
    write(&root, &format!("{folder}/provider.jadpo"), PROVIDER);
    write(
        &root,
        &format!("{folder}/types/send_email_request.jadpo"),
        REQUEST,
    );
    write(
        &root,
        &format!("{folder}/types/send_email_response.jadpo"),
        RESPONSE,
    );
    write(&root, &format!("{folder}/resources/emails.jadpo"), EMAILS);
    write(
        &root,
        "domains/notices/actions/send_notice.jadpo",
        "use app { mail }\nuse shared/providers/mailer\n\n/// Sends one notice.\naction send_notice(input: Mailer.SendEmailRequest) -> Mailer.SendEmailResponse {\n    return mail.emails.send(input)\n}\n",
    );
    root
}

fn checked(root: &Path) -> String {
    plain(root, &["records", "update"]);
    super::support::without_run_time(&plain(root, &["check"]).stdout)
}

#[test]
fn a_hand_written_mail_request_is_typed_marked_and_reviewed() {
    let dir = tempdir::Dir::new("type-review-app");
    let root = app(&dir);
    let folder = "shared/providers/mailer/v1";
    // Its text, read by name: two addresses and two of a person's words.
    let first = checked(&root);
    for expected in [
        "error: `SendEmailRequest.from` may be an `Email`\n",
        "error: `SendEmailRequest.to` may be an `Email`\n",
        "error: `SendEmailRequest.subject` may hold personal data with no `personal:`\n",
        "error: `SendEmailRequest.html` may hold personal data with no `personal:`\n",
        "check failed: 4 errors, 8 files\n",
    ] {
        assert!(first.contains(expected), "{expected}{first}");
    }
    assert!(
        first.contains("then make it `from: Email`, or `EmailWithName` where the provider shows a name with the address"),
        "{first}"
    );
    // Typed and marked, the types its call reaches are to review, though
    // it was written by hand.
    write(
        &root,
        &format!("{folder}/types/send_email_request.jadpo"),
        FIXED,
    );
    assert_eq!(
        checked(&root),
        "To review
  Go through each type against the provider's documentation, then add it to
  `reviewed:` in its package's `provider.jadpo` (agent)
    Mailer.v1  SendEmailRequest, SendEmailResponse; 0 of 2 reviewed

Not yet confirmed
  Record each with `jadpo package record`, which checks a sandbox answer
  against its types (agent)
    mail.emails.send

checked: 2 to review, 1 unconfirmed, 8 files
"
    );
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("build/report/providers.json")).expect("the report"),
    )
    .expect("JSON");
    assert_eq!(report["providers"][0]["imported"], false);
    assert_eq!(
        report["providers"][0]["types"][0],
        serde_json::json!({ "name": "SendEmailRequest", "reviewed": false })
    );
    // Its own `reviewed:`, in `provider { }`.
    write(
        &root,
        &format!("{folder}/provider.jadpo"),
        &PROVIDER.replace(
            "\n}\n",
            "\n    reviewed: [SendEmailRequest, SendEmailResponse]\n}\n",
        ),
    );
    assert_eq!(
        checked(&root),
        "Not yet confirmed
  Record each with `jadpo package record`, which checks a sandbox answer
  against its types (agent)
    mail.emails.send

checked: 1 unconfirmed, 8 files
"
    );
}

/// The package in its own repository, ready to publish but for its
/// review.
fn repository(dir: &tempdir::Dir) -> PathBuf {
    let root = dir.path().join("mailer");
    write(
        &root,
        "package.jadpo",
        "/// A mail API, written by hand from its documentation.\npackage Mailer.v1 {\n    publisher: \"@acme\"\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    );
    write(&root, "provider.jadpo", PROVIDER);
    write(&root, "types/send_email_request.jadpo", FIXED);
    write(&root, "types/send_email_response.jadpo", RESPONSE);
    write(&root, "resources/emails.jadpo", EMAILS);
    write(
        &root,
        "CHANGELOG.md",
        "# Changelog\n\n## 1.0.0\n\n- first release\n\nSends email.\n",
    );
    root
}

fn reviewed_row(root: &Path, args: &[&str]) -> (serde_json::Value, serde_json::Value) {
    let ran = plain(root, args);
    let document: serde_json::Value = serde_json::from_str(&ran.stdout)
        .unwrap_or_else(|_| panic!("JSON: {}{}", ran.stdout, ran.stderr));
    let row = document["data"]["bar"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["item"] == "reviewed"))
        .cloned()
        .unwrap_or_else(|| panic!("a reviewed row: {document}"));
    (row, document["diagnostics"].clone())
}

#[test]
fn the_bar_holds_every_public_type_to_the_review() {
    let dir = tempdir::Dir::new("type-review-bar");
    let root = repository(&dir);
    let checked = plain(&root, &["package", "check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked
            .stdout
            .contains("  reviewed      failed                        2 not in `reviewed:`\n"),
        "{}",
        checked.stdout
    );
    assert!(
        checked
            .stdout
            .contains("error: Public type `SendEmailRequest` isn't reviewed\n"),
        "{}",
        checked.stdout
    );
    // The registry's verifier runs the same bar, and its JSON names each
    // type and the step.
    let shown = root.to_str().expect("a path");
    let (row, diagnostics) = reviewed_row(dir.path(), &["package", "verify", shown, "--json"]);
    assert_eq!(row["state"], "failed", "{row}");
    assert_eq!(
        row["detail"],
        "`SendEmailRequest` and `SendEmailResponse` aren't reviewed: go through each against the provider's documentation, then add it to `reviewed:` in `provider.jadpo`"
    );
    let rules: Vec<&str> = diagnostics
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|d| d["rule"].as_str())
        .filter(|rule| *rule == "publish.type_unreviewed")
        .collect();
    assert_eq!(rules.len(), 2, "{diagnostics}");
    // One reviewed: the other is named alone.
    write(
        &root,
        "provider.jadpo",
        &PROVIDER.replace("\n}\n", "\n    reviewed: [SendEmailRequest]\n}\n"),
    );
    let (row, _) = reviewed_row(&root, &["package", "check", "--json"]);
    assert_eq!(
        row["detail"],
        "`SendEmailResponse` isn't reviewed: go through it against the provider's documentation, then add it to `reviewed:` in `provider.jadpo`"
    );
    // Both: the row passes, and so does the bar.
    write(
        &root,
        "provider.jadpo",
        &PROVIDER.replace(
            "\n}\n",
            "\n    reviewed: [SendEmailRequest, SendEmailResponse]\n}\n",
        ),
    );
    let (row, _) = reviewed_row(&root, &["package", "check", "--json"]);
    assert_eq!(row["state"], "passed", "{row}");
    assert_eq!(row["detail"], "every public type");
    let checked = plain(&root, &["package", "check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
}

#[test]
fn a_library_has_no_review_row() {
    let dir = tempdir::Dir::new("type-review-library");
    let root = dir.path().join("words");
    write(
        &root,
        "package.jadpo",
        "/// Words.\npackage Words.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    );
    write(
        &root,
        "types/word.jadpo",
        "/// One word.\npublic type Word = Text {\n    max_length: 40\n}\n",
    );
    let ran = plain(&root, &["package", "check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&ran.stdout).expect("JSON");
    let rows = document["data"]["bar"].as_array().expect("rows");
    assert!(
        !rows.iter().any(|row| row["item"] == "reviewed"),
        "{rows:?}"
    );
}
