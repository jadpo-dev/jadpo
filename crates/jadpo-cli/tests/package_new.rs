//! `jadpo package new <Name> --library` or `--provider` (area 11, "Developing
//! and publishing a package"; area 14, "`jadpo package new`"): what it writes
//! passes `jadpo package check` as it is, and passes `verify` once its author
//! has chosen a licence.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use jadpo_cli::{Session, run};
use jadpo_term::{Environment, Mode, Style};

use super::http::{jadpo, tempdir, write};
use super::packages::plain;

fn bar_rows(root: &Path) -> serde_json::Value {
    let checked = jadpo(root, &["package", "check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&checked.stdout)
        .unwrap_or_else(|_| panic!("JSON: {}{}", checked.stdout, checked.stderr));
    document["data"]["bar"].clone()
}

fn row<'a>(rows: &'a serde_json::Value, item: &str) -> &'a serde_json::Value {
    rows.as_array()
        .and_then(|rows| rows.iter().find(|row| row["item"] == item))
        .unwrap_or_else(|| panic!("a {item} row: {rows}"))
}

/// Every file under `root`, with its text, hidden ones among them.
fn tree(root: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(folder).expect("a folder").flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else {
                let text = std::fs::read_to_string(&path).expect("text");
                out.push((path, text));
            }
        }
    }
    out.sort();
    out
}

/// The package named `code`, of `kind`, started for `@acme`.
fn started(tag: &str, code: &str, kind: &str) -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new(tag);
    let made = jadpo(
        dir.path(),
        &["package", "new", code, kind, "--publisher", "@acme"],
    );
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    let folder = made
        .stdout
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("Created "))
        .expect("the folder it made")
        .to_string();
    let root = dir.path().join(folder);
    (dir, root)
}

#[test]
fn a_library_is_written_as_the_screen_says() {
    let dir = tempdir::Dir::new("package-new-library");
    let made = jadpo(
        dir.path(),
        &[
            "package",
            "new",
            "AcmePricing",
            "--library",
            "--publisher",
            "@acme",
        ],
    );
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    let (major, minor) = jadpo_check::jadpo_version::this_line();
    assert_eq!(
        made.stdout,
        format!(
            "Created acme-pricing
    package    AcmePricing.v1, library
    publisher  @acme
    licence    proprietary, publishing needs an open one
    jadpo      {major}.{minor}
    files      15 written

next: cd acme-pricing && jadpo package check  runs the publishing bar on it
"
        )
    );
    let root = dir.path().join("acme-pricing");
    let index = std::fs::read_to_string(root.join("package.jadpo")).expect("the index");
    assert_eq!(
        index,
        format!(
            "/// AcmePricing: say here, in one sentence, what this package does.
package AcmePricing.v1 {{
    publisher: \"@acme\"
    licence: Licence.proprietary
    jadpo: \"{major}.{minor}\"
}}
"
        )
    );
    for path in [
        "GUIDE.md",
        "CHANGELOG.md",
        "surface.jadpo_record",
        "sample/app.jadpo",
        "recipes/tidy_entered_text/actions/tidy_entered_text.jadpo",
        "libraries/starter.jadpo",
        "libraries/starter.test.jadpo",
        ".gitignore",
    ] {
        assert!(root.join(path).is_file(), "{path}");
    }
    assert!(!root.join("provider.jadpo").exists());
}

#[test]
fn what_it_writes_passes_the_check_as_it_is() {
    for (tag, code, kind, expected) in [
        (
            "package-new-check-library",
            "Postcodes",
            "--library",
            &["compiles", "described", "examples", "tests", "recipes"][..],
        ),
        (
            "package-new-check-provider",
            "AcmeCouriers",
            "--provider",
            &["compiles", "described", "reviewed", "recipes"][..],
        ),
    ] {
        let (_dir, root) = started(tag, code, kind);
        let before = tree(&root);
        let checked = plain(&root, &["package", "check"]);
        assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
        assert!(
            !checked.stdout.contains("not checked yet"),
            "{}",
            checked.stdout
        );
        // It carries all `check` would write, so there is nothing to change.
        assert!(!checked.stdout.contains("Wrote"), "{}", checked.stdout);
        assert_eq!(tree(&root), before);
        let rows = bar_rows(&root);
        for item in expected {
            assert_eq!(row(&rows, item)["state"], "passed", "{item}: {rows}");
        }
        assert_eq!(row(&rows, "publisher")["detail"], "@acme", "{rows}");
    }
}

#[test]
fn a_provider_says_where_its_placeholders_are() {
    let (_dir, root) = started("package-new-provider", "AcmeCouriers", "--provider");
    let provider = std::fs::read_to_string(root.join("provider.jadpo")).expect("the provider");
    assert!(
        provider.contains("https://api.acme-couriers.example"),
        "{provider}"
    );
    // Nothing is claimed as confirmed against an API nobody has called.
    assert!(!root.join("recordings").exists());
    assert!(root.join("sample/config.jadpo").is_file());
    assert!(!root.join("libraries").exists());
}

#[test]
fn it_passes_the_registrys_bar_once_its_licence_is_chosen() {
    for (tag, code, kind) in [
        ("package-new-verify-library", "Postcodes", "--library"),
        ("package-new-verify-provider", "AcmeCouriers", "--provider"),
    ] {
        let (dir, root) = started(tag, code, kind);
        let shown = root.to_str().expect("a path");
        let refused = plain(dir.path(), &["package", "verify", shown]);
        assert_eq!(refused.code, 1, "{}", refused.stdout);
        assert!(
            refused
                .stdout
                .contains("rule: publish.licence_proprietary\n"),
            "{}",
            refused.stdout
        );
        let index = std::fs::read_to_string(root.join("package.jadpo")).expect("the index");
        write(
            &root,
            "package.jadpo",
            &index.replace("Licence.proprietary", "Licence.mit"),
        );
        let checked = plain(&root, &["package", "check"]);
        assert_eq!(checked.code, 0, "{}", checked.stdout);
        let verified = plain(dir.path(), &["package", "verify", shown]);
        assert_eq!(verified.code, 0, "{}", verified.stdout);
    }
}

#[test]
fn what_it_writes_is_formatted() {
    for (tag, code, kind) in [
        ("package-new-fmt-library", "Postcodes", "--library"),
        ("package-new-fmt-provider", "AcmeCouriers", "--provider"),
    ] {
        let (dir, root) = started(tag, code, kind);
        // `fmt` reads a project: the package as an app's own integration, and
        // the sample as the app it is.
        let app = dir.path().join("app");
        write(&app, "app.jadpo", "application {}\n");
        let snake = jadpo_db::to_snake(code);
        let mount = app.join("shared/providers").join(&snake).join("v1");
        for (path, text) in tree(&root) {
            let inside = path.strip_prefix(&root).expect("inside");
            if inside.starts_with("sample") {
                continue;
            }
            write(&mount, inside.to_str().expect("a path"), &text);
        }
        for project in [app, root.join("sample")] {
            let formatted = plain(&project, &["fmt", "--check"]);
            assert_eq!(
                formatted.code, 0,
                "{}{}",
                formatted.stdout, formatted.stderr
            );
        }
    }
}

#[test]
fn names_work_or_are_refused_with_nothing_left() {
    for (code, folder) in [
        ("GitHub", "git-hub"),
        ("Oauth2", "oauth2"),
        ("Match", "match"),
    ] {
        let (_dir, root) = started(&format!("package-new-name-{folder}"), code, "--provider");
        assert_eq!(root.file_name().expect("a name"), folder);
        let checked = plain(&root, &["package", "check"]);
        assert_eq!(checked.code, 0, "{code}: {}", checked.stdout);
    }
    let dir = tempdir::Dir::new("package-new-bad-names");
    for code in ["postcodes", "Acme_Pay", "ACME", "Postcodes.v2", "Acme-Pay"] {
        let refused = jadpo(dir.path(), &["package", "new", code, "--library"]);
        assert_eq!(refused.code, 2, "{code}: {}", refused.stdout);
        assert!(
            refused
                .stderr
                .starts_with(&format!("error: `{code}` isn't a package's name\n")),
            "{}",
            refused.stderr
        );
        assert!(refused.stderr.contains("rule: cli.package_arguments\n"));
    }
    let long = format!("A{}", "b".repeat(39));
    let refused = jadpo(dir.path(), &["package", "new", &long, "--library"]);
    assert_eq!(refused.code, 2, "{}", refused.stdout);
    // A name only the compiler can turn down leaves nothing behind.
    for code in ["Text", "Instant"] {
        let refused = jadpo(dir.path(), &["package", "new", code, "--library"]);
        if refused.code != 0 {
            assert!(
                refused
                    .stderr
                    .starts_with(&format!("error: `{code}` can't name a package\n")),
                "{}",
                refused.stderr
            );
            assert!(!dir.path().join(jadpo_db::to_snake(code)).exists());
        }
    }
    assert_eq!(
        std::fs::read_dir(dir.path())
            .expect("the folder")
            .flatten()
            .filter(|entry| entry.path().is_file())
            .count(),
        0
    );
}

#[test]
fn it_says_what_it_needs() {
    let dir = tempdir::Dir::new("package-new-arguments");
    let cases: [(&[&str], &str); 7] = [
        (
            &["package", "new"],
            "error: `jadpo package new` needs a name for the package\n",
        ),
        (
            &["package", "new", "Postcodes"],
            "error: `jadpo package new` needs `--library` or `--provider`\n",
        ),
        (
            &["package", "new", "Postcodes", "--library", "--provider"],
            "error: A package can't be both `--library` and `--provider`\n",
        ),
        (
            &["package", "new", "Postcodes", "Other", "--library"],
            "error: `jadpo package new` takes one name, not `Other` as well\n",
        ),
        (
            &[
                "package",
                "new",
                "Postcodes",
                "--library",
                "--publisher",
                "acme",
            ],
            "error: `acme` isn't a publisher's handle\n",
        ),
        (
            &["package", "check", "--library"],
            "error: `--library` goes with `jadpo package new`\n",
        ),
        (
            &["package", "show", "x", "--publisher", "@acme"],
            "error: `--publisher` goes with `verify` and `new`\n",
        ),
    ];
    for (args, said) in cases {
        let refused = jadpo(dir.path(), args);
        assert_eq!(refused.code, 2, "{args:?}: {}", refused.stdout);
        assert!(
            refused.stderr.starts_with(said),
            "{args:?}: {}",
            refused.stderr
        );
    }
    assert_eq!(
        std::fs::read_dir(dir.path()).expect("the folder").count(),
        0
    );
}

#[test]
fn a_folder_with_anything_in_it_is_left_alone() {
    let dir = tempdir::Dir::new("package-new-folder");
    write(dir.path(), "postcodes/.keep", "");
    let refused = jadpo(dir.path(), &["package", "new", "Postcodes", "--library"]);
    assert_eq!(refused.code, 1, "{}", refused.stdout);
    assert!(
        refused
            .stderr
            .contains("rule: toolchain.scaffold_destination_not_empty\n"),
        "{}",
        refused.stderr
    );
    assert!(
        refused.stderr.contains("`jadpo package new`"),
        "{}",
        refused.stderr
    );
    assert_eq!(tree(&dir.path().join("postcodes")).len(), 1);

    write(dir.path(), "pricing", "a file\n");
    let refused = jadpo(dir.path(), &["package", "new", "Pricing", "--library"]);
    assert!(
        refused
            .stderr
            .contains("rule: toolchain.scaffold_destination_exists\n"),
        "{}",
        refused.stderr
    );
}

#[test]
fn elsewhere_than_a_terminal_it_leaves_the_key_out() {
    let dir = tempdir::Dir::new("package-new-no-terminal");
    let made = jadpo(dir.path(), &["package", "new", "Postcodes", "--library"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    assert!(
        made.stdout
            .contains("publisher  left out, publishing needs it\n"),
        "{}",
        made.stdout
    );
    let root = dir.path().join("postcodes");
    let index = std::fs::read_to_string(root.join("package.jadpo")).expect("the index");
    assert!(!index.contains("publisher"), "{index}");
    let checked = plain(&root, &["package", "check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    assert_eq!(row(&bar_rows(&root), "publisher")["state"], "left_out");
    let shown = root.to_str().expect("a path");
    let refused = plain(dir.path(), &["package", "verify", shown]);
    assert!(
        refused.stdout.contains("rule: publish.publisher_missing\n"),
        "{}",
        refused.stdout
    );
}

/// `jadpo package new` with `args`, every stream a terminal.
fn on_a_terminal(cwd: &Path, args: &[&str]) -> jadpo_cli::Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(
        &args,
        &Environment::empty()
            .with_cwd(cwd)
            .with_terminals(true, true, true),
    )
}

#[test]
fn on_a_terminal_it_asks_who_publishes_it() {
    let dir = tempdir::Dir::new("package-new-terminal");
    let asks = on_a_terminal(dir.path(), &["package", "new", "Postcodes", "--library"]);
    let Some(Session::PackageNew(args)) = &asks.session else {
        panic!("it asks: {}{}", asks.stdout, asks.stderr);
    };
    assert!(!dir.path().join("postcodes").exists(), "nothing yet");

    // Given a handle, or asked for JSON, there is nothing to ask.
    let given = on_a_terminal(
        dir.path(),
        &[
            "package",
            "new",
            "Given",
            "--library",
            "--publisher",
            "@acme",
        ],
    );
    assert!(given.session.is_none());
    let document = on_a_terminal(
        dir.path(),
        &["package", "new", "Quiet", "--provider", "--json"],
    );
    assert!(document.session.is_none());
    let document: serde_json::Value = serde_json::from_str(&document.stdout).expect("JSON");
    assert_eq!(document["data"]["package"], "Quiet.v1");
    assert_eq!(document["data"]["kind"], "provider");
    assert_eq!(document["data"]["licence"], "proprietary");
    assert_eq!(document["data"]["publisher"], serde_json::Value::Null);

    // What it reads: a wrong answer is asked again, and the answer goes in
    // the index.
    let style = Style::new(Mode::Plain);
    let mut said = Vec::new();
    let answer =
        jadpo_cli::package_new::ask_publisher(&mut Cursor::new("acme\n@acme\n"), &mut said, &style);
    assert_eq!(answer.as_deref(), Some("@acme"));
    let said = String::from_utf8(said).expect("text");
    assert!(said.contains("Who publishes this package?"), "{said}");
    assert!(said.contains("`acme` isn't a publisher's handle"), "{said}");
    assert!(!said.contains('\x1b'), "plain output moves nothing: {said}");
    jadpo_cli::package_new::write(args, answer.as_deref());
    let index =
        std::fs::read_to_string(dir.path().join("postcodes/package.jadpo")).expect("the index");
    assert!(index.contains("publisher: \"@acme\""), "{index}");

    // Enter, or the input ending, leaves it out.
    for typed in ["\n", ""] {
        let answer =
            jadpo_cli::package_new::ask_publisher(&mut Cursor::new(typed), &mut Vec::new(), &style);
        assert_eq!(answer, None);
    }
    // Answered on a rich terminal, the prompt collapses to a record of it.
    let mut said = Vec::new();
    jadpo_cli::package_new::ask_publisher(
        &mut Cursor::new("@acme\n"),
        &mut said,
        &Style::new(Mode::Rich),
    );
    let said = String::from_utf8(said).expect("text");
    assert!(
        said.contains("✓ Who publishes this package?  @acme"),
        "{said}"
    );
    // In colour, the symbols carry it, and the question is what to read first.
    let mut colour = Style::new(Mode::Rich);
    colour.color = Some(jadpo_term::Depth::Ansi16);
    colour.bold = true;
    let mut said = Vec::new();
    jadpo_cli::package_new::ask_publisher(&mut Cursor::new("\n"), &mut said, &colour);
    let said = String::from_utf8(said).expect("text");
    assert!(
        said.starts_with("\x1b[36m?\x1b[0m \x1b[1mWho publishes this package?\x1b[0m  \x1b["),
        "{said:?}"
    );
}
