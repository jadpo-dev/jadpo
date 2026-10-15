//! `jadpo fmt` (synonym `format`) and `jadpo fmt --check` (area 14): the
//! project is found by walking up to `app.jadpo`, a file or folder can be
//! named instead, and every result is a screen in the design language or a
//! `--json` document.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

const FORMATTED_APP: &str = "application {\n    revocation_delay: 5m\n}\n";
const MESSY_COUNT: &str = "type Count = Int { min: 0 }\n";
const TIDY_COUNT: &str = "type Count = Int {\n    min: 0\n}\n";
const COUNT: &str = "domains/planning/types/count.jadpo";

/// A fresh folder for one test under cargo's temporary directory.
mod support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("fmt-{name}"));
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
    fs::read_to_string(root.join(path)).expect("read")
}

/// A project with one formatted and one messy file, and files `fmt` skips.
fn project(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", FORMATTED_APP);
    write(&root, COUNT, MESSY_COUNT);
    write(&root, "build/generated.jadpo", "type   X = Y {}\n");
    write(&root, ".editor/scratch.jadpo", "type   X = Y {}\n");
    root
}

fn strip_bold(text: &str) -> String {
    text.replace("\x1b[1m", "").replace("\x1b[0m", "")
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

#[test]
fn fmt_finds_the_project_from_a_subfolder_and_formats_every_file() {
    let root = project("finds");
    let output = jadpo(&root.join("domains/planning"), &["fmt"]);
    assert_eq!(output.stderr, "");
    assert_eq!(output.stdout, "formatted: 1 changed, 1 unchanged\n");
    assert_eq!(output.code, 0);
    assert_eq!(read(&root, COUNT), TIDY_COUNT);
    assert_eq!(read(&root, "build/generated.jadpo"), "type   X = Y {}\n");
    assert_eq!(read(&root, ".editor/scratch.jadpo"), "type   X = Y {}\n");
    let again = jadpo(&root, &["fmt"]);
    assert_eq!(again.stdout, "formatted: 2 unchanged\n");
}

#[test]
fn format_is_a_synonym() {
    let root = project("synonym");
    assert_eq!(
        jadpo(&root, &["format"]).stdout,
        "formatted: 1 changed, 1 unchanged\n"
    );
}

#[test]
fn check_reports_each_unformatted_file_and_changes_nothing() {
    let root = project("check");
    let output = jadpo(&root, &["fmt", "--check"]);
    assert_eq!(output.code, 1);
    assert_eq!(output.stderr, "");
    assert_eq!(
        output.stdout,
        "error: This file isn't formatted
 --> domains/planning/types/count.jadpo:1:20
  |
1 | type Count = Int { min: 0 }
  |                    ^ the formatter changes this line
  reason: `jadpo fmt` lays this file out differently from here on.
  next: Run `jadpo fmt` to format it (agent)
  rule: format.changes_required
  help: docs/language/14-tooling.html#formatting

fmt failed: 1 not formatted, 1 formatted
"
    );
    assert_eq!(read(&root, COUNT), MESSY_COUNT);
    jadpo(&root, &["fmt"]);
    let clean = jadpo(&root, &["fmt", "--check"]);
    assert_eq!(clean.stdout, "formatted: 2 files\n");
    assert_eq!(clean.code, 0);
}

#[test]
fn rich_check_output_uses_symbols_and_the_footer() {
    let root = project("rich");
    let env = Environment::empty()
        .with_cwd(&root)
        .with_terminals(true, true, true)
        .with_var("TERM", "xterm-256color")
        .with_var("NO_COLOR", "1");
    let mut output = run(&["fmt".to_string(), "--check".to_string()], &env);
    output.stdout = strip_bold(&output.stdout);
    assert!(
        output
            .stdout
            .starts_with("✗ error  This file isn't formatted\n"),
        "{}",
        output.stdout
    );
    assert!(
        output
            .stdout
            .ends_with("\n✗ fmt failed  1 not formatted · 1 formatted\n"),
        "{}",
        output.stdout
    );
}

#[test]
fn a_file_with_syntax_errors_is_reported_and_left_alone() {
    let root = project("syntax");
    write(
        &root,
        "domains/planning/types/bad.jadpo",
        "type Count = Int {\n    min: 0 $\n}\n",
    );
    let output = jadpo(&root, &["fmt"]);
    assert_eq!(output.code, 1);
    assert_eq!(
        output.stdout,
        "error: `$` isn't part of Jadpo
 --> domains/planning/types/bad.jadpo:2:12
  |
2 |     min: 0 $
  |            ^ not Jadpo
  reason: Jadpo source is names, literals, brackets and operators such as `+` and `==`; this character has no meaning.
  next: Remove `$` (agent)
  rule: syntax.unexpected_character
  help: docs/language/01-project-layout-and-modules.html#syntax-unexpected-character

fmt failed: 1 with syntax errors, 1 changed, 1 unchanged
"
    );
    assert_eq!(
        read(&root, COUNT),
        TIDY_COUNT,
        "other files are still formatted"
    );
}

#[test]
fn outside_a_project_fmt_says_so() {
    let dir = folder("outside");
    let output = jadpo(&dir, &["fmt"]);
    assert_eq!(output.code, 1);
    assert_eq!(
        output.stdout,
        "error: No Jadpo project here
  reason: Jadpo looks for `app.jadpo` in this folder and every folder above it, and found none.
  next: Run `jadpo fmt` inside a project, or give a file or folder: `jadpo fmt <path>` (agent)
  rule: toolchain.project_not_found
  help: docs/language/14-tooling.html#the-jadpo-command
"
    );
}

#[test]
fn a_named_file_or_folder_is_formatted_on_its_own() {
    let root = project("named");
    let output = jadpo(&root, &["fmt", COUNT, "--check"]);
    assert!(
        output
            .stdout
            .contains(" --> domains/planning/types/count.jadpo:1:20\n"),
        "{}",
        output.stdout
    );
    assert!(output.stdout.ends_with("fmt failed: 1 not formatted\n"));
    let loose = folder("loose");
    write(&loose, "one.jadpo", MESSY_COUNT);
    write(&loose, "nested/two.jadpo", TIDY_COUNT);
    let output = jadpo(&root, &["fmt", loose.to_str().expect("path"), "--check"]);
    assert!(
        output.stdout.contains(" --> one.jadpo:1:20\n"),
        "{}",
        output.stdout
    );
    assert!(
        output
            .stdout
            .ends_with("fmt failed: 1 not formatted, 1 formatted\n")
    );
}

#[test]
fn a_folder_with_no_jadpo_files_has_nothing_to_format() {
    let dir = folder("empty");
    write(&dir, "README.md", "notes\n");
    let output = jadpo(&dir, &["fmt", "."]);
    assert_eq!(output.stdout, "nothing to format: no .jadpo files\n");
    assert_eq!(output.code, 0);
}

#[test]
fn json_lists_every_file_with_its_status() {
    let root = project("json");
    let output = jadpo(&root, &["fmt", "--check", "--json"]);
    assert_eq!(output.stderr, "");
    assert_eq!(output.code, 1);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    assert_eq!(document["command"], "fmt");
    assert_eq!(document["ok"], false);
    assert_eq!(
        document["data"],
        serde_json::json!({
            "check": true,
            "files": [
                {"path": "app.jadpo", "status": "unchanged"},
                {"path": COUNT, "status": "unformatted"}
            ]
        })
    );
    let diagnostic = &document["diagnostics"][0];
    assert_eq!(diagnostic["rule"], "format.changes_required");
    assert_eq!(diagnostic["location"]["path"], COUNT);
    assert_eq!(diagnostic["location"]["line"], 1);
    assert_eq!(diagnostic["location"]["column"], 20);
    assert_eq!(diagnostic["next"]["edits"][0]["replacement"], TIDY_COUNT);
}

#[test]
fn a_path_that_is_not_there_or_not_jadpo_is_a_usage_error() {
    let root = project("usage");
    let output = jadpo(&root, &["fmt", "nowhere.jadpo"]);
    assert_eq!(output.code, 2);
    assert_eq!(
        output.stderr,
        "error: `nowhere.jadpo` doesn't exist
  reason: There is no file or folder at this path.
  next: Give the path of a `.jadpo` file or a folder (agent)
  rule: cli.fmt_arguments
  help: docs/language/14-tooling.html#command-errors
"
    );
    write(&root, "notes.md", "notes\n");
    let output = jadpo(&root, &["fmt", "notes.md"]);
    assert_eq!(output.code, 2);
    assert!(
        output
            .stderr
            .starts_with("error: `notes.md` isn't a Jadpo file\n"),
        "{}",
        output.stderr
    );
    let output = jadpo(&root, &["fmt", "--chek"]);
    assert!(
        output.stderr.contains("Did you mean `--check`?"),
        "{}",
        output.stderr
    );
}

#[test]
fn help_describes_fmt() {
    let root = folder("help");
    let output = jadpo(&root, &["fmt", "--help"]);
    assert_eq!(
        output.stdout,
        "jadpo fmt, also called format
Rewrite Jadpo files in the one layout the formatter owns: indentation, spacing,
blank lines and one member per line. In a project it first fixes each file's
`use` lines: adds a missing import, removes an unused one and sorts them. With
--layout it also moves each declaration another file uses into a file of its
own, as layout.promotion_required asks. With --check, change nothing and fail if
any file would change. Without a path, the project is found by walking up to
app.jadpo.

Usage
  jadpo fmt [path]

Arguments
  [path]        A file or folder instead of the whole project

Flags
  --check       Report unformatted files and change nothing
  --layout      Also make the moves promotion asks for

On every command
  --json        Print data for tools instead of text
  --help        Show every flag, with examples
  --color       Choose when to use colour     auto, always, never

Examples
  Format the project you are in
    jadpo fmt

  Make the moves layout.promotion_required asks for, then format
    jadpo fmt --layout

  Check formatting without changing anything, as CI does
    jadpo fmt --check

  Format one file
    jadpo fmt app.jadpo
"
    );
}

#[test]
fn tab_completes_jadpo_files_and_folders_for_the_path() {
    let root = project("complete");
    let output = jadpo(&root, &["__complete", "--", "fmt", ""]);
    assert_eq!(
        output.stdout,
        "app.jadpo\tJadpo file\nbuild/\tFolder\ndomains/\tFolder\n"
    );
    let output = jadpo(&root, &["__complete", "--", "fmt", "domains/pl"]);
    assert_eq!(output.stdout, "domains/planning/\tFolder\n");
}

/// Area 1, Imports: inside a project, `jadpo fmt` adds a missing `use`
/// line, removes an unused one and sorts them; `--check` fails on a file
/// whose imports it would change.
#[test]
fn fmt_fixes_and_sorts_imports() {
    let root = folder("imports");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "shared/types/pence.jadpo",
        "type Pence = Int {\n    min: 0\n}\n",
    );
    write(
        &root,
        "shared/types/percent.jadpo",
        "type Percent = Int {\n    min: 0\n    max: 100\n}\n",
    );
    write(
        &root,
        "domains/bakery/types/loaf.jadpo",
        "type Loaf = Object {\n    price: Pence\n}\n",
    );
    write(
        &root,
        "domains/bakery/actions/price_loaf.jadpo",
        "use shared/types { Percent }\n\naction price_loaf(loaf: Loaf, each: Pence) -> Pence {\n    return each\n}\n",
    );
    write(
        &root,
        "domains/bakery/actions/discount_loaf.jadpo",
        "use shared/types { Percent }\n\naction discount_loaf(reduction: Percent) -> Unit {}\n",
    );
    let checked = jadpo(&root, &["fmt", "--check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    let output = jadpo(&root, &["fmt"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert_eq!(
        read(&root, "domains/bakery/actions/price_loaf.jadpo"),
        "use shared/types { Pence }\nuse bakery/types { Loaf }\n\naction price_loaf(loaf: Loaf, each: Pence) -> Pence {\n    return each\n}\n"
    );
    assert_eq!(
        read(&root, "domains/bakery/types/loaf.jadpo"),
        "use shared/types { Pence }\n\ntype Loaf = Object {\n    price: Pence\n}\n"
    );
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    assert_eq!(jadpo(&root, &["fmt", "--check"]).code, 0);
}

/// `jadpo fmt --layout` makes the moves `layout.promotion_required` asks
/// for and fixes the imports they touch, in one step (area 1, "Sharing and
/// promotion"); `--check --layout` only reports them.
#[test]
fn fmt_layout_moves_a_shared_type_into_a_file_of_its_own() {
    let root = folder("layout");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/planning/actions/snooze_todo.jadpo",
        "/// Days to snooze for.\ntype SnoozeDays = Int {\n    min: 1\n    max: 30\n}\n\naction snooze_todo(days: SnoozeDays) -> Unit {}\n",
    );
    write(
        &root,
        "domains/planning/actions/postpone_all.jadpo",
        "action postpone_all(days: SnoozeDays) -> Unit {}\n",
    );
    let pending = jadpo(&root, &["fmt", "--check", "--layout"]);
    assert_eq!(pending.code, 1);
    assert!(
        pending.stdout.contains("1 move to make"),
        "{}",
        pending.stdout
    );
    assert!(
        !root
            .join("domains/planning/types/snooze_days.jadpo")
            .exists(),
        "--check changes nothing"
    );
    let output = jadpo(&root, &["fmt", "--layout"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert_eq!(
        read(&root, "domains/planning/types/snooze_days.jadpo"),
        "/// Days to snooze for.\ntype SnoozeDays = Int {\n    min: 1\n    max: 30\n}\n"
    );
    assert_eq!(
        read(&root, "domains/planning/actions/snooze_todo.jadpo"),
        "use planning/types { SnoozeDays }\n\naction snooze_todo(days: SnoozeDays) -> Unit {}\n"
    );
    assert_eq!(
        read(&root, "domains/planning/actions/postpone_all.jadpo"),
        "use planning/types { SnoozeDays }\n\naction postpone_all(days: SnoozeDays) -> Unit {}\n"
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
}
