//! A package's conformance runs as a release shows them (area 11, "Proving
//! a package works" and "What a release shows"; area 14, "`jadpo package
//! check`" and "`jadpo package verify`"; chunk K4b): `jadpo package check`
//! and the verifier run the suite of each capability the package fills,
//! against its recordings and a local fake on this machine's loopback
//! address, in the run that runs its code. The verifier's answer pins each
//! run to the release's digest, which is what the registry lists a package
//! for a capability by. Nothing leaves the machine.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("conformance-runs-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("a folder");
    for entry in fs::read_dir(from).expect("the fixture") {
        let entry = entry.expect("an entry");
        let path = entry.path();
        if path.is_dir() {
            copy(&path, &to.join(entry.file_name()));
        } else {
            fs::copy(&path, to.join(entry.file_name())).expect("a copy");
        }
    }
}

/// A copy of fixture package `name`, since `check` writes its surface.
fn package(test: &str, name: &str) -> PathBuf {
    let root = folder(test);
    let to = root.join(name);
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/packages")
            .join(name),
        &to,
    );
    to
}

fn document(out: &Output) -> serde_json::Value {
    serde_json::from_str(&out.stdout).unwrap_or_else(|_| panic!("JSON: {}", out.stdout))
}

/// A run's capability, whether it conforms and each case's state.
type Run = (String, bool, Vec<(String, String)>);

/// Each run's capability, whether it conforms and each case's state.
fn runs(out: &Output) -> Vec<Run> {
    document(out)["data"]["conformance"]
        .as_array()
        .unwrap_or_else(|| panic!("no conformance: {}", out.stdout))
        .iter()
        .map(|run| {
            (
                run["capability"].as_str().unwrap_or_default().to_string(),
                run["conforms"] == true,
                run["cases"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|case| {
                        (
                            case["case"].as_str().unwrap_or_default().to_string(),
                            case["state"].as_str().unwrap_or_default().to_string(),
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

fn passed(cases: &[&str]) -> Vec<(String, String)> {
    cases
        .iter()
        .map(|case| (case.to_string(), "passed".to_string()))
        .collect()
}

#[test]
fn check_runs_the_suite_of_each_capability_the_package_fills() {
    let resend = package("check-resend", "resend");
    let out = jadpo(&resend, &["package", "check", "--json"]);
    assert_eq!(
        runs(&out),
        vec![(
            "email".to_string(),
            true,
            passed(&["message", "text_only", "attachment", "rejection"])
        )]
    );
    let run = &document(&out)["data"]["conformance"][0];
    assert_eq!(run["against"], "recordings", "{run}");
    // Only the verifier knows the release's digest.
    assert_eq!(run["digest"], serde_json::Value::Null, "{run}");

    let screen = jadpo(&resend, &["package", "check"]);
    assert!(
        screen
            .stdout
            .contains("  conformance: email: conforms, 4 passed\n"),
        "{}",
        screen.stdout
    );
}

#[test]
fn verify_runs_the_suites_and_its_no_run_pass_leaves_them_to_the_run() {
    let sqs = package("verify-sqs", "amazon_sqs");
    let out = jadpo(
        &sqs,
        &["package", "verify", sqs.to_str().expect("a path"), "--json"],
    );
    assert_eq!(
        runs(&out),
        vec![(
            "receiving".to_string(),
            true,
            passed(&["messages", "empty_queue", "acknowledge"])
        )]
    );
    let source = jadpo(
        &sqs,
        &[
            "package",
            "verify",
            sqs.to_str().expect("a path"),
            "--no-run",
            "--json",
        ],
    );
    assert_eq!(
        document(&source)["data"]["conformance"],
        serde_json::Value::Null,
        "{}",
        source.stdout
    );
}

/// A suite that fails is shown, and doesn't refuse the release: the
/// registry leaves the package out of the capability's listing instead.
#[test]
fn a_suite_that_fails_is_shown_and_leaves_the_bar_as_it_was() {
    let sqs = package("check-sqs-unrecorded", "amazon_sqs");
    let before = jadpo(&sqs, &["package", "check", "--json"]);
    fs::remove_file(sqs.join("recordings/messages.receive_message.json")).expect("removed");
    let out = jadpo(&sqs, &["package", "check", "--json"]);
    let runs = runs(&out);
    assert_eq!(runs.len(), 1, "{}", out.stdout);
    assert_eq!(runs[0].0, "receiving");
    assert!(!runs[0].1, "{}", out.stdout);
    assert!(
        runs[0].2.iter().any(|(_, state)| state == "failed"),
        "{}",
        out.stdout
    );
    let bar = |out: &Output| {
        document(out)["data"]["bar"]
            .as_array()
            .expect("the bar")
            .iter()
            .filter(|row| row["item"] != "decoders")
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(bar(&out), bar(&before));
    assert!(
        !out.stdout.contains("conform.failed"),
        "a case's problem isn't the bar's: {}",
        out.stdout
    );

    let screen = jadpo(&sqs, &["package", "check"]);
    assert!(
        screen
            .stdout
            .contains("  conformance: receiving: doesn't conform, "),
        "{}",
        screen.stdout
    );
}

/// A package that fills no capability has nothing to run.
#[test]
fn a_package_that_fills_nothing_runs_no_suite() {
    let sqs = package("check-fills-nothing", "amazon_sqs");
    fs::remove_dir_all(sqs.join("provides")).expect("removed");
    let out = jadpo(&sqs, &["package", "check", "--json"]);
    assert_eq!(runs(&out), Vec::new(), "{}", out.stdout);
    let screen = jadpo(&sqs, &["package", "check"]);
    assert!(!screen.stdout.contains("conformance"), "{}", screen.stdout);
}
