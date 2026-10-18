//! `jadpo suggest` (area 14, "Suggestions"; chunk 5.5x): the numbered
//! findings and the agent's routine, `--catalogue`, a finding taken when
//! it disappears, `ignore` keeping a snippet and its variants quiet until
//! it changes, `found` keeping a miss's text through the fix and teaching
//! it to the next scan, and `share`, which isn't built yet.

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

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
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
  When done, run `jadpo suggest share`.

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
  When done, run `jadpo suggest share`.

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
        record.starts_with("// Written by `jadpo suggest ignore`"),
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
fn share_is_not_built_yet() {
    let root = shop("share");
    let output = jadpo(&root, &["suggest", "share"]);
    assert_ne!(output.code, 0);
    assert!(
        output.stderr.contains("toolchain.not_built_yet"),
        "{}",
        output.stderr
    );
    assert!(
        output.stderr.contains("isn't built yet"),
        "{}",
        output.stderr
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
