//! `jadpo suggest` (area 14, "Suggestions"; chunk 5.5x): the numbered
//! findings and the agent's routine, `--catalogue`, a finding taken when
//! it disappears, `ignore` keeping a snippet and its variants quiet until
//! it changes, and `found` keeping a miss's text through the fix and
//! teaching it to the next scan; `share` is in `suggest_share`.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("suggest-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap_or_default()
}

/// `jadpo`, never asking the public registry: the registry's own tests
/// give it a local one ([`with_registry`]).
fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    jadpo_in(
        cwd,
        args,
        &Environment::empty().with_var("JADPO_SUGGEST_REGISTRY", "off"),
    )
}

fn jadpo_in(cwd: &Path, args: &[&str], env: &Environment) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &env.clone().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// `jadpo` asking the registry at `url`.
fn with_registry(url: &str, cwd: &Path, args: &[&str]) -> Output {
    jadpo_in(
        cwd,
        args,
        &Environment::empty().with_var("JADPO_REGISTRY", url),
    )
}

fn json(cwd: &Path, args: &[&str]) -> serde_json::Value {
    let mut args = args.to_vec();
    args.push("--json");
    let output = jadpo(cwd, &args);
    serde_json::from_str(&output.stdout).expect("JSON")
}

const WHOLE: (&str, &str) = (
    "domains/shop/types/whole.jadpo",
    "/// A whole number.\ntype Whole = Int {}\n",
);

fn pence_of(body: &str) -> String {
    format!(
        "use shop/types {{ Whole }}\n\n/// Pence from a price in pence.\naction pence_of(total_price: Whole) -> Whole {{\n    return {body}\n}}\n"
    )
}

/// A shop whose action converts a price by hand.
fn shop(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", "application {}\n");
    write(&root, WHOLE.0, WHOLE.1);
    write(
        &root,
        "domains/shop/actions/pence_of.jadpo",
        &pence_of("Whole(total_price * 100)"),
    );
    root
}

/// The pending entries, each as `kind signal suggestion: text`.
fn pending(root: &Path) -> Vec<String> {
    read(root, ".jadpo/suggest_pending")
        .lines()
        .map(|line| {
            let entry: serde_json::Value = serde_json::from_str(line).expect("a JSON line");
            format!(
                "{} {} {}: {}",
                entry["kind"].as_str().unwrap_or_default(),
                entry["signal"].as_str().unwrap_or_default(),
                entry["suggestion"].as_str().unwrap_or_default(),
                entry["text"].as_str().unwrap_or_default()
            )
        })
        .collect()
}

#[test]
fn it_numbers_each_finding_and_ends_with_the_routine() {
    let root = shop("scan");
    let output = jadpo(&root, &["suggest"]);
    assert_eq!(output.stderr, "");
    assert_eq!(
        output.stdout,
        "Suggestions
  S1  domains/shop/actions/pence_of.jadpo:4  Money  name
      total_price: Whole
  S2  domains/shop/actions/pence_of.jadpo:5  Money  conversion
      total_price * 100

What to do
  Review the rest of the code against `jadpo suggest --catalogue`.
  Before changing code for something it missed, run
  `jadpo suggest found <file>:<line> <Type or operation> \"<why>\"`.
  To keep one quiet, run `jadpo suggest ignore <number> \"<why>\"`.

2 suggestions: 1 file
"
    );
    assert_eq!(output.code, 0);
    let document = json(&root, &["suggest"]);
    let second = &document["data"]["findings"][1];
    assert_eq!(second["number"], "S2");
    assert_eq!(second["signal"], "conversion");
    assert_eq!(second["suggestion"], "Money");
    assert_eq!(second["text"], "total_price * 100");
    assert_eq!(second["canonical"], "(* 100 total_price)");
    assert_eq!(second["hash"].as_str().map(str::len), Some(16));
    assert!(
        read(&root, ".gitignore")
            .lines()
            .any(|line| line == ".jadpo/")
    );
}

#[test]
fn with_nothing_to_suggest_the_routine_stays() {
    let root = folder("none");
    write(&root, "app.jadpo", "application {}\n");
    write(&root, WHOLE.0, WHOLE.1);
    let output = jadpo(&root, &["suggest"]);
    assert_eq!(
        output.stdout,
        "What to do
  Review the rest of the code against `jadpo suggest --catalogue`.
  Before changing code for something it missed, run
  `jadpo suggest found <file>:<line> <Type or operation> \"<why>\"`.

No suggestions
"
    );
}

#[test]
fn a_finding_that_disappears_was_taken() {
    let root = shop("taken");
    jadpo(&root, &["suggest"]);
    write(
        &root,
        "domains/shop/actions/pence_of.jadpo",
        &pence_of("total_price"),
    );
    let output = jadpo(&root, &["suggest"]);
    assert!(
        output
            .stdout
            .ends_with("1 suggestion: 1 file, 1 taken since the last run\n"),
        "{}",
        output.stdout
    );
    assert_eq!(
        pending(&root),
        ["accepted conversion Money: total_price * 100"]
    );
}

#[test]
fn an_ignored_snippet_and_its_variants_stay_quiet_until_it_changes() {
    let root = shop("ignore");
    jadpo(&root, &["suggest"]);
    let output = jadpo(
        &root,
        &["suggest", "ignore", "S2", "Basis points, not money"],
    );
    assert_eq!(output.stderr, "");
    assert_eq!(
        output.stdout,
        "Ignored S2, Money conversion
    snippet  total_price * 100
    place    domains/shop/actions/pence_of.jadpo:5
    reason   Basis points, not money
"
    );
    let record = read(&root, "ignored_suggestions.jadpo_record");
    assert!(
        record.starts_with("// Generated by Jadpo from the suggestions this project ignores; update it with `jadpo suggest ignore`, never by hand.\nrecord ignored_suggestions 2\n"),
        "{record}"
    );
    assert!(
        record.contains(" conversion Money \"Basis points, not money\"\n"),
        "{record}"
    );
    assert_eq!(
        pending(&root),
        ["ignored conversion Money: total_price * 100"]
    );

    // Quiet, and not taken.
    let quiet = json(&root, &["suggest"]);
    assert_eq!(quiet["data"]["findings"].as_array().map(Vec::len), Some(1));
    assert_eq!(quiet["data"]["ignored"], 1);
    assert_eq!(quiet["data"]["accepted"], 0);

    // Written the other way round, it is the same snippet.
    write(
        &root,
        "domains/shop/actions/pence_of.jadpo",
        &pence_of("Whole(100 * total_price)"),
    );
    let variant = json(&root, &["suggest"]);
    assert_eq!(variant["data"]["ignored"], 1);
    assert_eq!(
        variant["data"]["findings"].as_array().map(Vec::len),
        Some(1)
    );

    // Changed, it is found again and the stale entry drops out.
    write(
        &root,
        "domains/shop/actions/pence_of.jadpo",
        &pence_of("Whole(total_price * 1000)"),
    );
    let changed = json(&root, &["suggest"]);
    assert_eq!(changed["data"]["ignored"], 0);
    assert_eq!(changed["data"]["staleIgnores"], 1);
    assert_eq!(changed["data"]["findings"][1]["text"], "total_price * 1000");
    assert!(!read(&root, "ignored_suggestions.jadpo_record").contains("ignore "));
}

#[test]
fn a_found_snippet_keeps_its_text_through_the_fix_and_teaches_the_next_scan() {
    let root = folder("found");
    write(&root, "app.jadpo", "application {}\n");
    write(&root, WHOLE.0, WHOLE.1);
    let total = "use shop/types { Whole }\n\n/// An order's total.\naction total_of(subtotal: Whole, delivery: Whole) -> Whole {\n    return Whole(subtotal + delivery)\n}\n";
    write(&root, "domains/shop/actions/total_of.jadpo", total);
    let output = jadpo(
        &root,
        &[
            "suggest",
            "found",
            "domains/shop/actions/total_of.jadpo:5",
            "Money",
            "Pence held as Int",
        ],
    );
    assert_eq!(output.stderr, "");
    assert_eq!(output.code, 0);
    assert!(
        output
            .stdout
            .starts_with("Recorded a missed Money at domains/shop/actions/total_of.jadpo:5\n    snippet  subtotal + delivery\n    reason   Pence held as Int\n"),
        "{}",
        output.stdout
    );
    // The fix changes the code; the text recorded before it stays.
    write(
        &root,
        "domains/shop/actions/total_of.jadpo",
        "use shop/types { Whole }\n\n/// An order's total.\naction total_of(subtotal: Whole) -> Whole {\n    return subtotal\n}\n",
    );
    assert_eq!(pending(&root), ["found taught Money: subtotal + delivery"]);
    // The same pattern elsewhere, written the other way round.
    write(
        &root,
        "domains/shop/actions/charge_of.jadpo",
        "use shop/types { Whole }\n\n/// What to charge.\naction charge_of(subtotal: Whole, delivery: Whole) -> Whole {\n    return Whole(delivery + subtotal)\n}\n",
    );
    let next = json(&root, &["suggest"]);
    let findings = next["data"]["findings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0]["signal"], "taught");
    assert_eq!(findings[0]["suggestion"], "Money");
    assert_eq!(findings[0]["text"], "delivery + subtotal");
}

#[test]
fn the_catalogue_lists_every_signal() {
    let root = folder("catalogue");
    let output = jadpo(&root, &["suggest", "--catalogue"]);
    assert_eq!(output.code, 0);
    assert!(
        output.stdout.starts_with("Catalogue\n"),
        "{}",
        output.stdout
    );
    assert!(output.stdout.contains("list.sum"), "{}", output.stdout);
    let document = json(&root, &["suggest", "--catalogue"]);
    let signals = document["data"]["signals"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(signals.iter().any(|s| s["suggestion"] == "Money"
        && s["signal"] == "conversion"
        && s["values"] == serde_json::json!(["100", "1000"])));
    assert!(
        signals
            .iter()
            .any(|s| s["suggestion"] == "Point" && s["signal"] == "shape")
    );
}

#[test]
fn wrong_arguments_say_what_is_meant() {
    let root = shop("arguments");
    let output = jadpo(&root, &["suggest", "ignor"]);
    assert!(
        output.stderr.contains("cli.suggest_arguments"),
        "{}",
        output.stderr
    );
    assert!(
        output.stderr.contains("Did you mean `ignore`?"),
        "{}",
        output.stderr
    );
    jadpo(&root, &["suggest"]);
    let output = jadpo(&root, &["suggest", "ignore", "S9", "why"]);
    assert!(output.stderr.contains("S1 or S2"), "{}", output.stderr);
    let output = jadpo(&root, &["suggest", "ignore", "S1"]);
    assert!(
        output.stderr.contains("needs a reason"),
        "{}",
        output.stderr
    );
    let output = jadpo(
        &root,
        &[
            "suggest",
            "found",
            "domains/shop/actions/pence_of.jadpo:5",
            "Mony",
            "why",
        ],
    );
    assert!(
        output.stderr.contains("Did you mean `Money`?"),
        "{}",
        output.stderr
    );
    let output = jadpo(
        &root,
        &[
            "suggest",
            "found",
            "domains/shop/actions/pence_of.jadpo:2",
            "Money",
            "why",
        ],
    );
    assert!(output.stderr.contains("has nothing"), "{}", output.stderr);
}

// --- the registry (roadmap R8) ----------------------------------------------

use std::sync::{Arc, Mutex};

use jadpo_registry::http::{Incoming, Server};
use jadpo_registry::local::{LocalRegistry, ReleaseSeed};
use jadpo_registry::names::{Digest, ScopedName, Version};
use jadpo_registry::protocol::PublishedFingerprint;

/// `uk.parse`'s source fingerprint, as the verifier writes it for the
/// registry: `jadpo package verify --json` on the package.
fn published_parse() -> String {
    let (_dir, package) = crate::packages::repository("suggest-published");
    let verified = run(
        &[
            "package".to_string(),
            "verify".to_string(),
            package.display().to_string(),
            "--no-run".to_string(),
            "--json".to_string(),
        ],
        &Environment::empty().with_cwd(&package),
    );
    let document: serde_json::Value = serde_json::from_str(&verified.stdout)
        .unwrap_or_else(|_| panic!("JSON: {}{}", verified.stdout, verified.stderr));
    let prints = document["data"]["fingerprints"]
        .as_array()
        .unwrap_or_else(|| panic!("fingerprints: {document}"));
    prints
        .iter()
        .find(|print| print["declaration"] == "uk.parse")
        .and_then(|print| print["source"].as_str())
        .unwrap_or_else(|| panic!("uk.parse: {prints:?}"))
        .to_string()
}

/// A registry holding `@acme/postcodes` 2.1.0, whose `uk.parse` has
/// `source`, behind a stand-in that keeps every body it is sent.
fn registry_with(source: &str) -> (LocalRegistry, Server, String, Arc<Mutex<Vec<String>>>) {
    let registry = LocalRegistry::new();
    registry.publisher("@acme", true, false);
    let package = ScopedName::parse("@acme/postcodes").unwrap();
    let version = Version::parse("2.1.0").unwrap();
    registry.release(ReleaseSeed::new(
        package.clone(),
        version.clone(),
        vec![("package.jadpo".into(), b"package Postcodes.v2\n".to_vec())],
    ));
    registry.fingerprint(PublishedFingerprint {
        package,
        version,
        declaration: "uk.parse".into(),
        source: Digest::parse(source).unwrap(),
    });
    let sent = Arc::new(Mutex::new(Vec::new()));
    let kept = sent.clone();
    let behind = registry.clone();
    let server = Server::start(Arc::new(move |incoming: Incoming| {
        kept.lock().unwrap().push(format!(
            "{} {} {}",
            incoming.method,
            incoming.target,
            String::from_utf8_lossy(&incoming.body)
        ));
        behind.handle(&incoming)
    }))
    .expect("serves");
    let url = format!("http://{}/registry/v1/", server.address());
    (registry, server, url, sent)
}

/// A shop holding the package's types and its own copy of `uk.parse`,
/// renamed, with its variable renamed too.
fn copying_shop(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/shop/types/uk_postcode.jadpo",
        "/// A UK postcode in standard form, such as \"SW1A 1AA\".\ntype UkPostcode = Text {\n    min_length: 5\n    max_length: 8\n}\n",
    );
    write(
        &root,
        "domains/shop/types/written_postcode.jadpo",
        "/// A postcode as someone typed it.\ntype WrittenPostcode = Text {\n    max_length: 20\n}\n",
    );
    write(
        &root,
        "domains/shop/failures/invalid_postcode.jadpo",
        "/// The text isn't a UK postcode.\ntype InvalidPostcode = Failure {\n    kind: FailureKind.invalid_value\n}\n",
    );
    write(
        &root,
        "domains/shop/libraries/addresses.jadpo",
        "use shop/types { UkPostcode, WrittenPostcode }\nuse shop/failures { InvalidPostcode }\n\n/// Reading addresses.\nlibrary addresses {\n    /// Reads the postcode a customer typed.\n    function read_postcode(typed: WrittenPostcode) fails InvalidPostcode -> UkPostcode {\n        return attempt UkPostcode(text.upper(typed)) {\n            invalid: InvalidPostcode\n        }\n    }\n}\n",
    );
    write(
        &root,
        "domains/shop/actions/check_postcode.jadpo",
        "use shop/types { UkPostcode, WrittenPostcode }\nuse shop/failures { InvalidPostcode }\nuse shop/libraries { addresses }\n\n/// Checks the postcode a customer typed.\naction check_postcode(written: WrittenPostcode) fails InvalidPostcode -> UkPostcode {\n    return attempt addresses.read_postcode(written)\n}\n",
    );
    root
}

#[test]
fn a_copy_of_a_published_function_is_found_through_the_registry() {
    let source = published_parse();
    let (_registry, _server, url, sent) = registry_with(&source);
    let root = copying_shop("published");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);

    let scanned = with_registry(&url, &root, &["suggest"]);
    assert_eq!(scanned.code, 0, "{}{}", scanned.stdout, scanned.stderr);
    assert_eq!(
        scanned.stdout,
        "Suggestions
  S1  domains/shop/libraries/addresses.jadpo:7  @acme/postcodes uk.parse  published
      function read_postcode(typed: WrittenPostcode) fails InvalidPostcode -> U…

What to do
  Review the rest of the code against `jadpo suggest --catalogue`.
  Before changing code for something it missed, run
  `jadpo suggest found <file>:<line> <Type or operation> \"<why>\"`.
  To keep one quiet, run `jadpo suggest ignore <number> \"<why>\"`.
  To use a published one, run `jadpo package add <package>` and call it.

1 suggestion: 1 file
"
    );
    // Only the first five hex digits of the fingerprint went, never the
    // fingerprint, a name or the source.
    let sent = sent.lock().unwrap().clone();
    let hex = &source["sha256:".len()..];
    assert_eq!(sent.len(), 1, "{sent:?}");
    let (target, body) = sent[0].split_once(" {").expect("a body");
    assert_eq!(target, "POST /registry/v1/fingerprints/query");
    let body: serde_json::Value = serde_json::from_str(&format!("{{{body}")).expect("JSON");
    let prefixes: Vec<&str> = body["prefixes"]
        .as_array()
        .expect("prefixes")
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    assert!(prefixes.contains(&&hex[..5]), "{prefixes:?}");
    assert!(prefixes.iter().all(|p| p.len() == 5), "{prefixes:?}");
    assert_eq!(body.as_object().unwrap().len(), 1, "{body}");

    let document: serde_json::Value =
        serde_json::from_str(&with_registry(&url, &root, &["suggest", "--json"]).stdout)
            .expect("JSON");
    let data = &document["data"];
    assert_eq!(data["registry"]["state"], "answered");
    let finding = &data["findings"][0];
    assert_eq!(finding["signal"], "published");
    assert_eq!(finding["suggestion"], "@acme/postcodes uk.parse");
    assert_eq!(finding["package"], "@acme/postcodes");
    assert_eq!(finding["version"], "2.1.0");
    assert!(
        finding["reason"].as_str().unwrap().contains("copy"),
        "{finding}"
    );
}

#[test]
fn without_the_registry_a_copy_is_neither_taken_nor_its_ignore_dropped() {
    let source = published_parse();
    let (_registry, server, url, _sent) = registry_with(&source);
    let root = copying_shop("published-offline");
    let first = with_registry(&url, &root, &["suggest"]);
    assert!(first.stdout.contains("published"), "{}", first.stdout);
    let ignored = with_registry(
        &url,
        &root,
        &[
            "suggest",
            "ignore",
            "S1",
            "A trimmed copy we keep on purpose",
        ],
    );
    assert_eq!(ignored.code, 0, "{}{}", ignored.stdout, ignored.stderr);
    let quiet = with_registry(&url, &root, &["suggest"]);
    assert!(
        quiet.stdout.contains("1 ignored"),
        "{}{}{}",
        ignored.stdout,
        quiet.stdout,
        read(&root, "ignored_suggestions.jadpo_record")
    );

    // Out of reach: the scan still runs and says so, and the ignore stays.
    drop(server);
    let offline = with_registry(&url, &root, &["suggest"]);
    assert_eq!(offline.code, 0, "{}{}", offline.stdout, offline.stderr);
    assert!(
        offline
            .stdout
            .ends_with("\nNo suggestions: registry not reached\n"),
        "{}",
        offline.stdout
    );
    assert!(
        read(&root, "ignored_suggestions.jadpo_record").contains("published"),
        "{}",
        read(&root, "ignored_suggestions.jadpo_record")
    );
    // Turned off, it isn't asked, and nothing is taken either.
    let off = json(&root, &["suggest"]);
    assert_eq!(off["data"]["registry"]["state"], "off");
    assert_eq!(off["data"]["accepted"], 0);
    assert!(
        pending(&root)
            .iter()
            .all(|entry| !entry.starts_with("accepted published"))
    );
}

#[test]
fn a_registry_that_doesnt_serve_the_query_yet_says_so() {
    use jadpo_registry::http::Fetched;

    let server = Server::start(Arc::new(|_: Incoming| Fetched {
        status: 404,
        content_type: Some("text/html".into()),
        body: b"<h1>Not found</h1>".to_vec(),
    }))
    .expect("serves");
    let url = format!("http://{}/registry/v1/", server.address());
    let root = copying_shop("published-unserved");
    let output = with_registry(&url, &root, &["suggest", "--json"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    assert_eq!(document["data"]["registry"]["state"], "unreachable");
    assert_eq!(
        document["data"]["registry"]["reason"],
        "It doesn't answer `POST fingerprints/query` yet."
    );
}
