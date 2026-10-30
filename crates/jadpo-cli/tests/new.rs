//! `jadpo new <name>` (area 14, "`jadpo new`"): writes a project that
//! passes `check`, `fmt --check` and `test` as soon as it is written, on
//! this jadpo's own release line, and refuses a folder that already holds
//! anything.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::support;

/// A fresh, empty folder for one test under cargo's temporary directory.
fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("new-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_str(&output.stdout).expect("one JSON document")
}

/// This jadpo's release line: its version's major and minor numbers.
fn line() -> String {
    let mut parts = env!("CARGO_PKG_VERSION").split('.');
    format!(
        "{}.{}",
        parts.next().expect("major"),
        parts.next().expect("minor")
    )
}

/// Every file under `root`, relative and sorted, and every empty folder
/// with a trailing `/`.
fn listing(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, found: &mut Vec<String>) {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .expect("read folder")
            .map(|entry| entry.expect("entry").path())
            .collect();
        entries.sort();
        let relative = |path: &Path| {
            path.strip_prefix(root)
                .expect("under root")
                .to_string_lossy()
                .replace('\\', "/")
        };
        if entries.is_empty() && dir != root {
            found.push(format!("{}/", relative(dir)));
        }
        for path in entries {
            if path.is_dir() {
                walk(root, &path, found);
            } else {
                found.push(relative(&path));
            }
        }
    }
    let mut found = Vec::new();
    walk(root, root, &mut found);
    found.sort();
    found
}

const WRITTEN: [&str; 14] = [
    ".gitattributes",
    ".gitignore",
    "AGENTS.md",
    "api/main.jadpo_record",
    "app.jadpo",
    "config.jadpo",
    "database/",
    "domains/hello/actions/greet.jadpo",
    "domains/hello/routes/hello.jadpo",
    "domains/hello/routes/hello.test.jadpo",
    "domains/hello/types/greeting.jadpo",
    "domains/hello/types/visitor.jadpo",
    "policy.jadpo",
    "tests/",
];

#[test]
fn a_new_project_passes_check_fmt_and_test_as_written() {
    let parent = folder("passes");
    let made = jadpo(&parent, &["new", "todo-app"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    let root = parent.join("todo-app");
    assert_eq!(listing(&root), WRITTEN);
    for args in [&["check"][..], &["fmt", "--check"], &["test"], &["records"]] {
        let output = jadpo(&root, args);
        assert_eq!(
            output.code, 0,
            "jadpo {args:?}:\n{}{}",
            output.stdout, output.stderr
        );
    }
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.stdout, "tested: 1 passed, 1 route test\n");
}

#[test]
fn the_project_is_on_this_jadpos_own_line() {
    let parent = folder("line");
    assert_eq!(jadpo(&parent, &["new", "shop"]).code, 0);
    let app = fs::read_to_string(parent.join("shop/app.jadpo")).expect("app.jadpo");
    assert!(
        app.starts_with(&format!("application {{\n    jadpo: \"{}\"\n}}\n", line())),
        "{app}"
    );
    let agents = fs::read_to_string(parent.join("shop/AGENTS.md")).expect("AGENTS.md");
    assert!(
        agents.contains(&format!("`jadpo: \"{}\"`", line())),
        "{agents}"
    );
    assert!(
        agents.contains("https://jadpo.dev/docs/primer.md"),
        "{agents}"
    );
}

#[test]
fn each_project_gets_its_own_token() {
    let parent = folder("token");
    assert_eq!(jadpo(&parent, &["new", "one"]).code, 0);
    assert_eq!(jadpo(&parent, &["new", "two"]).code, 0);
    let token = |name: &str| {
        let app = fs::read_to_string(parent.join(name).join("app.jadpo")).expect("app.jadpo");
        let start = app.find("project_token: \"").expect("a token") + 16;
        app[start..start + 32].to_string()
    };
    let (one, two) = (token("one"), token("two"));
    assert!(
        one.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    assert_ne!(one, two);
}

#[test]
fn the_record_is_the_one_api_update_writes() {
    let parent = folder("record");
    assert_eq!(jadpo(&parent, &["new", "todo-app"]).code, 0);
    let root = parent.join("todo-app");
    let updated = jadpo(&root, &["api", "update"]);
    assert_eq!(
        updated.stdout,
        "api/main.jadpo_record is up to date\n    version  6afd124d1e6c\n"
    );
}

#[test]
fn the_screen_says_what_was_made_and_the_one_command_to_run_next() {
    let parent = folder("screen");
    let made = jadpo(&parent, &["new", "todo-app"]);
    assert_eq!(made.stderr, "");
    assert_eq!(
        made.stdout,
        format!(
            "Created todo-app
    jadpo  {}
    route  POST /hello
    files  12 written

next: cd todo-app && jadpo dev  serves it on http://localhost:3000
",
            line()
        )
    );
}

#[test]
fn a_path_with_a_space_is_quoted_in_the_next_command() {
    let parent = folder("space");
    let made = jadpo(&parent, &["new", "my app"]);
    assert_eq!(made.code, 0, "{}", made.stderr);
    assert!(
        made.stdout
            .contains("next: cd 'my app' && jadpo dev  serves it"),
        "{}",
        made.stdout
    );
}

#[test]
fn json_lists_what_it_wrote() {
    let parent = folder("json");
    let document = json(&jadpo(&parent, &["new", "todo-app", "--json"]));
    assert_eq!(document["ok"], true);
    let data = &document["data"];
    assert_eq!(data["path"], "todo-app");
    assert_eq!(data["jadpo"], line());
    assert_eq!(data["routes"], serde_json::json!(["POST /hello"]));
    let files: Vec<&str> = data["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| file.as_str().expect("path"))
        .collect();
    let expected: Vec<&str> = WRITTEN
        .iter()
        .copied()
        .filter(|f| !f.ends_with('/'))
        .collect();
    assert_eq!(files, expected);
    assert_eq!(
        data["root"],
        parent.join("todo-app").to_string_lossy().as_ref()
    );
}

#[test]
fn an_empty_folder_and_the_current_one_can_be_the_destination() {
    let parent = folder("empty");
    fs::create_dir(parent.join("waiting")).expect("folder");
    assert_eq!(jadpo(&parent, &["new", "waiting"]).code, 0);
    assert!(parent.join("waiting/app.jadpo").is_file());
    let here = folder("here");
    let made = jadpo(&here, &["new", "."]);
    assert_eq!(made.code, 0, "{}", made.stderr);
    assert!(here.join("app.jadpo").is_file());
    assert!(made.stdout.starts_with("Created .\n"), "{}", made.stdout);
    assert!(
        made.stdout.contains("next: jadpo dev  serves it"),
        "{}",
        made.stdout
    );
    assert_eq!(jadpo(&here, &["check"]).code, 0);
}

#[test]
fn a_folder_that_holds_anything_is_refused_and_left_alone() {
    let parent = folder("not-empty");
    fs::create_dir(parent.join("todo-app")).expect("folder");
    fs::write(parent.join("todo-app/notes.txt"), "mine\n").expect("file");
    let refused = jadpo(&parent, &["new", "todo-app"]);
    assert_eq!(refused.code, 1);
    assert_eq!(refused.stdout, "");
    assert_eq!(
        refused.stderr,
        "error: `todo-app` already has files in it
  reason: `jadpo new` writes a whole project, so it writes only into a folder that doesn't exist yet or is empty. It wrote nothing.
  next: Name a folder that doesn't exist yet, or empty `todo-app` first (human)
  rule: toolchain.scaffold_destination_not_empty
  help: https://jadpo.dev/docs/errors/toolchain.scaffold_destination_not_empty
"
    );
    assert_eq!(listing(&parent.join("todo-app")), ["notes.txt"]);
    let document = json(&jadpo(&parent, &["new", "todo-app", "--json"]));
    assert_eq!(document["ok"], false);
    assert_eq!(
        document["diagnostics"][0]["rule"],
        "toolchain.scaffold_destination_not_empty"
    );
}

#[test]
fn a_file_in_the_way_is_refused() {
    let parent = folder("file");
    fs::write(parent.join("todo-app"), "a file\n").expect("file");
    let refused = jadpo(&parent, &["new", "todo-app"]);
    assert_eq!(refused.code, 1);
    assert!(
        refused.stderr.starts_with("error: `todo-app` is a file\n"),
        "{}",
        refused.stderr
    );
    assert!(
        refused
            .stderr
            .contains("rule: toolchain.scaffold_destination_exists")
    );
}

#[test]
fn a_destination_with_no_name_is_refused() {
    let parent = folder("no-name");
    for name in ["", ".."] {
        let refused = jadpo(&parent, &["new", name]);
        assert_eq!(refused.code, 1, "{name:?}");
        assert!(
            refused
                .stderr
                .contains("rule: toolchain.scaffold_name_invalid"),
            "{name:?}: {}",
            refused.stderr
        );
    }
}

#[test]
fn new_needs_exactly_one_name() {
    let parent = folder("arguments");
    let missing = jadpo(&parent, &["new"]);
    assert_eq!(missing.code, 2);
    assert!(
        missing.stderr.contains("rule: cli.new_arguments"),
        "{}",
        missing.stderr
    );
    let extra = jadpo(&parent, &["new", "one", "two"]);
    assert_eq!(extra.code, 2);
    assert!(
        extra.stderr.contains("rule: cli.new_arguments"),
        "{}",
        extra.stderr
    );
    assert_eq!(listing(&parent), Vec::<String>::new());
}

#[cfg(unix)]
#[test]
fn a_folder_that_cant_be_written_is_reported_and_nothing_is_left_behind() {
    use std::os::unix::fs::PermissionsExt;
    let parent = folder("read-only");
    fs::create_dir(parent.join("locked")).expect("folder");
    fs::set_permissions(parent.join("locked"), fs::Permissions::from_mode(0o555))
        .expect("read-only");
    let refused = jadpo(&parent, &["new", "locked"]);
    fs::set_permissions(parent.join("locked"), fs::Permissions::from_mode(0o755))
        .expect("writable again");
    assert_eq!(refused.code, 1);
    assert!(
        refused
            .stderr
            .contains("rule: toolchain.scaffold_write_failed"),
        "{}",
        refused.stderr
    );
    assert_eq!(listing(&parent.join("locked")), Vec::<String>::new());
}
