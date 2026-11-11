//! `jadpo test --affected` (area 14, "`jadpo test`" and "What `--affected`
//! measures against"): the tests that reach any node whose digest differs
//! from the base commit's, found as every diff finds its before side: the
//! merge base with the default branch, or on the default branch the
//! previous commit. Each test builds a git repository of its own.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::acceptances::git;
use crate::support;

const APP: &str = "application {}\n";
const COUNT: &str = "type Count = Int {\n    min: 0\n}\n";
const NORTH: &str = "domains/north/actions/north_add.jadpo";
const SOUTH: &str = "domains/south/actions/south_add.jadpo";

pub(crate) fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("affected-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

pub(crate) fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

/// An action in `domain` that adds, and a test beside it.
pub(crate) fn domain(root: &Path, domain: &str) {
    write(
        root,
        &format!("domains/{domain}/actions/{domain}_add.jadpo"),
        &format!(
            "use shared/types {{ Count }}\n\naction {domain}_add(a: Count, b: Count) -> Count {{\n    return Count(a + b)\n}}\n"
        ),
    );
    write(
        root,
        &format!("domains/{domain}/actions/{domain}_add.test.jadpo"),
        &format!(
            "use shared/types {{ Count }}\n\ntest \"{domain} adds\" {{\n    var got: Count = {domain}_add(1, 2)\n    assert got == 3\n}}\n"
        ),
    );
}

/// A project of two domains, each an action and its test.
pub(crate) fn project(root: &Path) {
    write(root, "app.jadpo", APP);
    write(root, "shared/types/count.jadpo", COUNT);
    domain(root, "north");
    domain(root, "south");
}

/// `name`'s project committed on `main`, then a branch `work` from it.
fn repository(name: &str) -> PathBuf {
    let root = folder(name);
    project(&root);
    // What `jadpo test` writes isn't part of the change.
    write(&root, ".gitignore", "build/\n");
    git(&root, &["init", "-q"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "first"]);
    git(&root, &["checkout", "-q", "-b", "work"]);
    root
}

fn short(root: &Path, reference: &str) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "--short=7", reference])
        .current_dir(root)
        .output()
        .expect("git runs");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub(crate) fn edit(root: &Path, path: &str, from: &str, to: &str) {
    let text = fs::read_to_string(root.join(path)).expect("read");
    assert!(text.contains(from), "{path} has no `{from}`");
    write(root, path, &text.replacen(from, to, 1));
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

#[test]
fn a_change_on_a_branch_runs_the_tests_it_reaches() {
    let root = repository("branch");
    let base = short(&root, "main");
    // Uncommitted, as an agent's work is before it commits.
    edit(&root, NORTH, "(a + b)", "(b + a)");
    let output = jadpo(&root, &["test", "--affected"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    assert_eq!(
        output.stdout,
        format!("tested: 1 passed, since main at {base}, 1 of 2 tests\n")
    );
    // Committed on the branch, it is still measured from the merge base.
    git(&root, &["commit", "-q", "-am", "north"]);
    edit(&root, SOUTH, "(a + b)", "(b + a)");
    let output = jadpo(&root, &["test", "--affected", "--json"]);
    let data: serde_json::Value = serde_json::from_str(&output.stdout).expect("json");
    assert_eq!(
        data["data"]["selection"]["affected"],
        format!("main at {base}")
    );
    assert_eq!(data["data"]["counts"]["ran"], 2);
    // Within what else is asked for.
    let output = jadpo(&root, &["test", "--affected", "--name", "south"]);
    assert_eq!(
        output.stdout,
        format!("tested: 1 passed, since main at {base}, 1 of 2 tests\n")
    );
}

#[test]
fn a_change_no_test_reaches_runs_nothing_and_passes() {
    let root = repository("nothing");
    let base = short(&root, "main");
    let output = jadpo(&root, &["test", "--affected"]);
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert_eq!(
        output.stdout,
        format!("nothing to run: no test reaches what changed since main at {base}\n")
    );
    let output = jadpo(&root, &["test", "--affected", "--json"]);
    let data: serde_json::Value = serde_json::from_str(&output.stdout).expect("json");
    assert_eq!(data["ok"], true);
    assert_eq!(
        data["data"],
        serde_json::json!({
            "selection": { "affected": format!("main at {base}") },
            "reached": 0,
            "tests": []
        })
    );
}

#[test]
fn on_the_default_branch_it_measures_from_the_previous_commit() {
    let root = repository("default");
    git(&root, &["checkout", "-q", "main"]);
    let previous = short(&root, "HEAD");
    edit(&root, SOUTH, "(a + b)", "(b + a)");
    git(&root, &["commit", "-q", "-am", "south"]);
    let output = jadpo(&root, &["test", "--affected", "--kind", "action"]);
    assert_eq!(
        output.stdout,
        format!("tested: 1 passed, since the previous commit, {previous}, 1 of 2 tests\n")
    );
}

#[test]
fn a_project_new_since_the_base_runs_every_test() {
    let root = folder("new");
    write(&root, "README.md", "Notes.\n");
    git(&root, &["init", "-q"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "first"]);
    git(&root, &["checkout", "-q", "-b", "work"]);
    let base = short(&root, "main");
    project(&root);
    let output = jadpo(&root, &["test", "--affected"]);
    assert_eq!(
        output.stdout,
        format!("tested: 2 passed, since main at {base}, 2 action tests\n")
    );
}

#[test]
fn without_a_base_commit_it_says_why() {
    // Outside any repository: the system's temporary folder, since
    // cargo's is inside this checkout.
    let root = std::env::temp_dir().join(format!("jadpo-affected-outside-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    project(&root);
    let output = jadpo(&root, &["test", "--affected"]);
    assert_eq!(output.code, 2);
    assert_eq!(
        output.stderr,
        "\
error: `--affected` has nothing to compare with
  reason: It runs the tests that reach what changed since the merge base with the default branch, and there is none: not a git repository.
  next: Run `jadpo test` to run every test (agent)
  rule: cli.test_arguments
  help: https://jadpo.dev/docs/errors/cli.test_arguments
"
    );
    let _ = fs::remove_dir_all(&root);
    let empty = folder("no-commit");
    project(&empty);
    git(&empty, &["init", "-q"]);
    let output = jadpo(&empty, &["test", "--affected"]);
    assert_eq!(output.code, 2);
    assert!(
        output
            .stderr
            .contains("and there is none: no commit yet.\n"),
        "{}",
        output.stderr
    );
}
