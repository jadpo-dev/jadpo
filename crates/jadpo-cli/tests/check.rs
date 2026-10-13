//! `jadpo check` (area 14): the project is found by walking up to
//! `app.jadpo`, a file or folder can be named instead, problems print in
//! the design language's error shape, and the last line is the summary.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

const APP: &str = "application {}\n";
const COUNT: &str = "domains/planning/types/count.jadpo";
const TALLY: &str = "domains/planning/actions/tally.jadpo";

/// A fresh folder for one test under cargo's temporary directory.
fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("check-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

/// A clean project of three files: a type in `types/` and an action using it.
fn project(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", APP);
    write(&root, COUNT, "type Count = Int {\n    min: 0\n}\n");
    write(
        &root,
        TALLY,
        "action tally(count: Count) -> Count {\n    return count\n}\n",
    );
    root
}

fn jadpo_in(env: Environment, cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &env.with_cwd(cwd))
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    jadpo_in(Environment::empty(), cwd, args)
}

fn terminal() -> Environment {
    Environment::empty()
        .with_terminals(true, true, true)
        .with_var("TERM", "xterm-256color")
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn a_clean_project_is_one_line_from_any_subfolder() {
    let root = project("clean");
    let output = jadpo(&root.join("domains/planning/actions"), &["check"]);
    assert_eq!(output.stdout, "checked: 3 files\n");
    assert_eq!(output.stderr, "");
    assert_eq!(output.code, 0);
}

const MISTAKE: &str = "action tally(count: Count) -> Count {\n    return dobule(count)\n}\n\nfunction double(count: Count) -> Count {\n    return count\n}\n";

#[test]
fn problems_print_in_the_error_shape_then_the_summary() {
    let root = project("problem");
    write(&root, TALLY, MISTAKE);
    let output = jadpo(&root, &["check"]);
    assert_eq!(
        output.stdout,
        "error: `dobule` isn't a function, action or query
 --> domains/planning/actions/tally.jadpo:2:12
  |
2 |     return dobule(count)
  |            ^^^^^^ not declared
  reason: Nothing in this project called `dobule` can be called. Did you mean `double`?
  next: Use `double` instead of `dobule` (agent)
  rule: semantic.unknown_callee
  help: docs/language/01-project-layout-and-modules.html#semantic-unknown-callee

check failed: 1 error, 3 files
"
    );
    assert_eq!(output.code, 1);
}

#[test]
fn a_terminal_gets_the_rich_form() {
    let root = project("rich");
    write(&root, TALLY, MISTAKE);
    let output = jadpo_in(terminal(), &root, &["check"]);
    assert!(output.stdout.contains("\x1b["), "a terminal gets colour");
    assert_eq!(
        strip_ansi(&output.stdout),
        "✗ error  `dobule` isn't a function, action or query
  domains/planning/actions/tally.jadpo:2:12
    │
  2 │     return dobule(count)
    │            ^^^^^^ not declared
    │
  reason    Nothing in this project called `dobule` can be called. Did you mean
            `double`?
  next      Use `double` instead of `dobule` (agent)
  rule      semantic.unknown_callee
  help      docs/language/01-project-layout-and-modules.html#semantic-unknown-callee

✗ check failed  1 error · 3 files
"
    );
}

#[test]
fn json_carries_the_problems_with_lines_and_edits() {
    let root = project("json");
    write(&root, TALLY, MISTAKE);
    let output = jadpo(&root, &["check", "--json"]);
    assert_eq!(output.code, 1);
    assert_eq!(output.stderr, "");
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    assert_eq!(document["schemaVersion"], 1);
    assert_eq!(document["command"], "check");
    assert_eq!(document["ok"], false);
    assert_eq!(document["data"]["layout"], "project");
    assert_eq!(
        document["data"]["files"],
        serde_json::json!(["app.jadpo", TALLY, COUNT])
    );
    assert_eq!(document["data"]["errors"], 1);
    let problem = &document["diagnostics"][0];
    assert_eq!(problem["rule"], "semantic.unknown_callee");
    assert_eq!(problem["location"]["path"], TALLY);
    assert_eq!(problem["location"]["line"], 2);
    assert_eq!(problem["location"]["column"], 12);
    assert_eq!(problem["next"]["edits"][0]["replacement"], "double");
    let again = jadpo(&root, &["check", "--json"]);
    assert_eq!(
        again.stdout, output.stdout,
        "the same source, the same bytes"
    );
}

#[test]
fn outside_a_project_a_named_file_is_checked_on_its_own() {
    let root = folder("loose");
    write(&root, "notes/sketch.jadpo", "entity Basket {}\n");
    write(&root, "notes/other.jadpo", "type Broken = Missing {}\n");
    let output = jadpo(&root, &["check", "notes/sketch.jadpo"]);
    assert_eq!(output.stdout, "checked: 1 file\n");
    let folder = jadpo(&root, &["check", "notes", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&folder.stdout).expect("JSON");
    assert_eq!(document["data"]["layout"], "loose");
    assert_eq!(document["diagnostics"][0]["rule"], "semantic.unknown_name");
}

#[test]
fn without_a_project_or_a_path_it_says_where_it_looked() {
    let root = folder("nowhere");
    let output = jadpo(&root, &["check"]);
    assert_eq!(
        output.stdout,
        "error: No Jadpo project here
  reason: Jadpo looks for `app.jadpo` in this folder and every folder above it, and found none.
  next: Run `jadpo check` inside a project, or give a file or folder: `jadpo check <path>` (agent)
  rule: toolchain.project_not_found
  help: docs/language/14-tooling.html#the-jadpo-command
"
    );
    assert_eq!(output.code, 1);
}

#[test]
fn a_path_that_does_not_exist_is_a_command_line_mistake() {
    let root = folder("missing");
    let output = jadpo(&root, &["check", "nowhere"]);
    assert_eq!(output.code, 2);
    assert!(
        output.stderr.starts_with("error: `nowhere` doesn't exist"),
        "{}",
        output.stderr
    );
    assert!(output.stderr.contains("rule: cli.check_arguments"));
}

#[test]
fn layout_problems_name_the_file_without_a_frame() {
    let root = project("layout");
    write(
        &root,
        "domains/planning/helpers/dates.jadpo",
        "type Day = Int {}\n",
    );
    let output = jadpo(&root, &["check"]);
    assert_eq!(
        output.stdout,
        "error: `helpers/` isn't a folder a domain has
 --> domains/planning/helpers/dates.jadpo
  reason: A domain holds only `entities/`, `types/`, `failures/`, `actions/`, `queries/`, `libraries/`, `routes/`, `schedules/`, `events/`, `subscribers/`, `emails/` and `fixtures/`, each named after what its files declare.
  next: Move the file to `domains/planning/types/day.jadpo` (agent)
  rule: layout.unknown_location
  help: docs/language/01-project-layout-and-modules.html#layout-unknown-location

check failed: 1 error, 4 files
"
    );
}

/// A domain folder that can't be listed fails the check rather than
/// passing a project with code missing (Phase 1 review, finding 22).
#[cfg(unix)]
#[test]
fn an_unreadable_folder_fails_the_check() {
    use std::os::unix::fs::PermissionsExt;
    let root = project("unreadable-folder");
    write(
        &root,
        "domains/billing/types/amount.jadpo",
        "type Amount = Int {}\n",
    );
    let locked = root.join("domains/billing");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
    let listable = fs::read_dir(&locked).is_ok();
    let output = jadpo(&root, &["check"]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("chmod back");
    if listable {
        // Running as root: every folder can be listed.
        return;
    }
    assert_eq!(output.code, 1, "{}", output.stdout);
    assert!(
        output
            .stdout
            .contains("`domains/billing/` couldn't be read"),
        "{}",
        output.stdout
    );
    assert!(output.stdout.contains("rule: toolchain.source_read_failed"));
}

/// A pipe named like a source file is reported, never read (it would block).
#[cfg(unix)]
#[test]
fn a_pipe_named_jadpo_is_not_read() {
    let root = project("fifo");
    let pipe = root.join("domains/planning/types/pipe.jadpo");
    let made = std::process::Command::new("mkfifo")
        .arg(&pipe)
        .status()
        .expect("mkfifo");
    assert!(made.success());
    let output = jadpo(&root, &["check"]);
    assert_eq!(output.code, 1, "{}", output.stdout);
    assert!(
        output
            .stdout
            .contains("`domains/planning/types/pipe.jadpo` couldn't be read"),
        "{}",
        output.stdout
    );
    let formatted = jadpo(&root, &["fmt", "--check"]);
    assert!(
        formatted.stdout.contains("isn't a regular file"),
        "{}",
        formatted.stdout
    );
}
