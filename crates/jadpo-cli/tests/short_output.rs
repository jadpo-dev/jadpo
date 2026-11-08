//! What a coding agent reads (chunks A13 and A14, from the 0.2.0 run): a
//! test database that can't start says so once, marked `(human)`; repeats
//! are shortened and `--full` brings them back; `--json` carries the
//! command's duration; the first `type.mismatch` shows the shape it wants
//! and its edit; a domain's rename is the agent's until the project has a
//! commit.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

const JADPO: &str = env!("CARGO_BIN_EXE_jadpo");

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("short-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("folder");
    for entry in fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy");
        }
    }
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_str(&output.stdout).expect("one JSON document")
}

/// `initdb` as a sandbox with no shared memory leaves it, as the 0.2.0 run
/// saw it, and the other tools a disposable server needs.
fn sandboxed_postgres(name: &str) -> PathBuf {
    let bin = folder(name);
    let initdb = "#!/bin/sh\ncat <<'END'\nThe files belonging to this database system will be owned by user \"someone\".\nThis user must also own the server process.\n\nselecting dynamic shared memory implementation ... sysv\nselecting default max_connections ... 20\nrunning bootstrap script ... 2026-11-01 21:54:11.133 GMT [20904] FATAL:  could not create shared memory segment: Operation not permitted\n2026-11-01 21:54:11.133 GMT [20904] DETAIL:  Failed system call was shmget(key=621383025, size=56, 03600).\nchild process exited with exit code 1\nEND\nexit 1\n";
    for (tool, script) in [
        ("initdb", initdb),
        ("pg_ctl", "#!/bin/sh\nexit 1\n"),
        ("postgres", "#!/bin/sh\nexit 1\n"),
    ] {
        let path = bin.join(tool);
        fs::write(&path, script).expect("tool");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("executable");
        }
    }
    bin
}

/// `jadpo test` in its own process, whose server is started from `bin`.
fn tests_with(bin: &Path, root: &Path, args: &[&str]) -> (i32, String) {
    let path = std::env::var("PATH").unwrap_or_default();
    let output = Command::new(JADPO)
        .arg("test")
        .args(args)
        .current_dir(root)
        .env("PATH", format!("{}:{path}", bin.display()))
        .env_remove("JADPO_TEST_DATABASE_URL")
        .env_remove("CI")
        .output()
        .expect("jadpo");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

/// A project with tables and action tests, copied from the examples.
fn stored_project(name: &str) -> PathBuf {
    let root = folder(name);
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/22-plant-helpline");
    copy(&example, &root);
    root
}

#[test]
fn a_test_database_that_cant_start_is_one_diagnostic_for_a_person() {
    let bin = sandboxed_postgres("sandbox-tools");
    let root = stored_project("sandbox-project");

    let (code, stdout) = tests_with(&bin, &root, &["--json"]);
    assert_eq!(code, 1, "{stdout}");
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("document");
    assert_eq!(document["ok"], false);
    let diagnostics = document["diagnostics"].as_array().expect("diagnostics");
    assert_eq!(diagnostics.len(), 1, "{stdout}");
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic["rule"], "test.database_unavailable");
    assert_eq!(diagnostic["next"]["decides"], "human");
    let next = diagnostic["next"]["text"].as_str().expect("next");
    assert!(next.contains("outside the sandbox"), "{next}");
    assert!(next.contains("JADPO_TEST_DATABASE_URL"), "{next}");
    let reason = diagnostic["reason"].as_str().expect("reason");
    assert!(
        reason.contains("could not create shared memory segment: Operation not permitted"),
        "{reason}"
    );
    assert!(!reason.contains("selecting default"), "{reason}");
    assert!(reason.contains("--full"), "{reason}");
    assert!(!stdout.contains("internal_fault"), "{stdout}");
    assert!(stdout.len() < 1500, "{} bytes: {stdout}", stdout.len());

    // All PostgreSQL said is there on request.
    let (_, full) = tests_with(&bin, &root, &["--json", "--full"]);
    let document: serde_json::Value = serde_json::from_str(&full).expect("document");
    let reason = document["diagnostics"][0]["reason"]
        .as_str()
        .expect("reason");
    assert!(
        reason.contains("selecting default max_connections"),
        "{reason}"
    );

    // One report on the screen too, and no test is said to have failed.
    let (code, screen) = tests_with(&bin, &root, &[]);
    assert_eq!(code, 1);
    assert_eq!(screen.matches("error:").count(), 1, "{screen}");
    assert!(!screen.contains("FAIL"), "{screen}");
    assert!(screen.contains("(human)"), "{screen}");
    assert!(
        screen.contains("rule: test.database_unavailable"),
        "{screen}"
    );
}

const SIZES: &str = "type Coins = Int {\n    min: 0\n}\n\ntype Count = Int {\n    min: 0\n}\n\nfunction line_total(price: Coins, quantity: Count) -> Coins {\n    return Coins(price * quantity)\n}\n";
const PRICED: &str = "test \"a large order is priced\" {\n    var price: Coins = 4611686018427387904\n    assert line_total(price, 2) > 0\n}\n\ntest \"a larger order is priced\" {\n    var price: Coins = 4611686018427387904\n    assert line_total(price, 3) > 0\n}\n\ntest \"a small order is priced\" {\n    var price: Coins = 2\n    assert line_total(price, 3) == 6\n}\n";

#[test]
fn a_fault_repeated_across_tests_is_shortened_and_full_brings_it_back() {
    let root = folder("repeated-fault");
    write(&root, "sizes.jadpo", SIZES);
    write(&root, "sizes.test.jadpo", PRICED);

    let output = jadpo(&root, &["test", ".", "--json", "--seed", "1"]);
    assert_eq!(output.code, 1);
    let document = json(&output);
    assert_eq!(document["schemaVersion"], 3);
    let data = &document["data"];
    assert_eq!(data["counts"]["ran"], 3);
    assert_eq!(data["counts"]["passed"], 1);
    assert_eq!(data["counts"]["failed"], 2);
    // Only what failed is listed.
    let tests = data["tests"].as_array().expect("tests");
    assert_eq!(tests.len(), 2, "{}", output.stdout);
    let first = &tests[0]["failure"]["fault"];
    assert!(first["stack"].is_array(), "{first}");
    assert!(first["reason"].is_string(), "{first}");
    let second = &tests[1]["failure"]["fault"];
    assert_eq!(second["sameAs"], tests[0]["order"], "{second}");
    assert_eq!(second["kind"], first["kind"]);
    assert!(second.get("reason").is_none(), "{second}");
    assert!(second.get("stack").is_none(), "{second}");
    assert!(tests[1]["failure"]["rerun"].is_string());
    assert!(tests[0]["failure"].get("assertion").is_none());
    assert!(tests[0]["failure"].get("kept").is_none());

    let full = json(&jadpo(
        &root,
        &["test", ".", "--json", "--seed", "1", "--full"],
    ));
    let tests = full["data"]["tests"].as_array().expect("tests");
    assert_eq!(tests.len(), 3);
    let faults: Vec<_> = tests
        .iter()
        .filter(|test| test["outcome"] == "failed")
        .collect();
    assert!(faults[1]["failure"]["fault"]["stack"].is_array());
    assert!(faults[1]["failure"]["fault"].get("sameAs").is_none());
    assert!(faults[0]["failure"]["assertion"].is_null());

    let screen = jadpo(&root, &["test", ".", "--seed", "1"]).stdout;
    assert_eq!(screen.matches("stack:").count(), 1, "{screen}");
    assert_eq!(
        screen.matches("As the first fault above").count(),
        1,
        "{screen}"
    );
    assert_eq!(screen.matches("rerun:").count(), 2, "{screen}");
    let whole = jadpo(&root, &["test", ".", "--seed", "1", "--full"]).stdout;
    assert_eq!(whole.matches("stack:").count(), 2, "{whole}");
}

#[test]
fn a_passing_run_counts_its_tests_and_full_lists_them() {
    let root = folder("passing");
    write(&root, "sizes.jadpo", SIZES);
    write(
        &root,
        "sizes.test.jadpo",
        "test \"a small order is priced\" {\n    var price: Coins = 2\n    assert line_total(price, 3) == 6\n}\n",
    );
    let output = jadpo(&root, &["test", ".", "--json"]);
    assert_eq!(output.code, 0);
    let document = json(&output);
    assert_eq!(document["data"]["counts"]["passed"], 1);
    assert!(document["data"].get("tests").is_none(), "{}", output.stdout);
    let full = json(&jadpo(&root, &["test", ".", "--json", "--full"]));
    assert_eq!(full["data"]["tests"].as_array().expect("tests").len(), 1);
}

#[test]
fn json_carries_the_commands_duration() {
    let root = folder("duration");
    write(&root, "sizes.jadpo", SIZES);
    for args in [
        &["check", "--json"][..],
        &["test", ".", "--json"][..],
        &["version", "--json"][..],
        &["check", "--nonsense", "--json"][..],
    ] {
        let document = json(&jadpo(&root, args));
        assert!(document["durationMs"].is_i64(), "{args:?} gave {document}");
    }
    // Not on a screen, whose time goes on its footer.
    assert!(!jadpo(&root, &["check"]).stdout.contains("durationMs"));
}

const BOOK: &str = "/// One physical copy.\nentity Book {\n    id: Uuid\n    author: PersonName\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n";
const ADD_BOOK: &str = "use lending/entities { Book }\n\n/// The details of a new copy.\ntype AddBook = Input {\n    pick: [Book.author]\n}\n\n/// Adds a copy.\naction add_book(input: AddBook) -> Book {\n    return Book.create {\n        set: {\n            author: input.author\n        }\n    }\n}\n";
const ADD_BOOK_TEST: &str = "test \"one\" {\n    var book: Book = add_book(AddBook {\n        author: \"Frank Herbert\"\n    })\n    assert book.author == book.author\n}\n\ntest \"two\" {\n    var book: Book = add_book(AddBook {\n        author: \"Ursula Le Guin\"\n    })\n    assert book.author == book.author\n}\n";

fn lending(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", "application {}\n");
    write(&root, "domains/lending/entities/book.jadpo", BOOK);
    write(&root, "domains/lending/actions/add_book.jadpo", ADD_BOOK);
    write(
        &root,
        "domains/lending/actions/add_book.test.jadpo",
        &format!("use lending/entities {{ Book }}\n\n{ADD_BOOK_TEST}"),
    );
    root
}

fn of_rule<'a>(document: &'a serde_json::Value, rule: &str) -> Vec<&'a serde_json::Value> {
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter(|diagnostic| diagnostic["rule"] == rule)
        .collect()
}

#[test]
fn the_first_mismatch_shows_the_shape_and_its_edit_and_the_next_points_back() {
    let root = lending("mismatch");
    let document = json(&jadpo(&root, &["check", "--json"]));
    let mismatches = of_rule(&document, "type.mismatch");
    assert_eq!(mismatches.len(), 2, "{document}");
    let first = mismatches[0];
    let reason = first["reason"].as_str().expect("reason");
    assert!(
        reason.contains("`PersonName { given: GivenName, family: FamilyName? }`"),
        "{reason}"
    );
    let next = first["next"]["text"].as_str().expect("next");
    let written = "Book.author(PersonName { given: \"Frank Herbert\", family: none })";
    assert!(next.contains(written), "{next}");
    assert!(next.contains("`jadpo help PersonName`"), "{next}");
    assert_eq!(first["next"]["edits"][0]["replacement"], written);

    // The second points at the first, and keeps its own edit.
    let second = mismatches[1];
    assert!(second.get("reason").is_none(), "{second}");
    assert!(second.get("help").is_none(), "{second}");
    assert!(second["sameAs"].is_i64(), "{second}");
    assert_eq!(
        second["next"]["edits"][0]["replacement"],
        "Book.author(PersonName { given: \"Ursula Le Guin\", family: none })"
    );

    // On the screen, and whole with `--full`.
    let screen = jadpo(&root, &["check"]).stdout;
    assert_eq!(
        screen.matches("As the first `type.mismatch` above").count(),
        1,
        "{screen}"
    );
    let whole = json(&jadpo(&root, &["check", "--json", "--full"]));
    let mismatches = of_rule(&whole, "type.mismatch");
    assert!(mismatches[1]["reason"].is_string());
    assert!(mismatches[1].get("sameAs").is_none());
    assert!(whole["data"]["files"].is_array());
    assert!(whole["data"]["build"].is_array());

    // Without `--full`, the lists of files are counted.
    assert!(document["data"]["fileCount"].is_i64(), "{document}");
    assert!(document["data"].get("files").is_none());
    assert!(document["data"].get("build").is_none());

    // The edits, applied, check: a field that may be `none` is still
    // written, so the next check finds nothing (area 2).
    let path = root.join("domains/lending/actions/add_book.test.jadpo");
    let mut text = fs::read_to_string(&path).expect("read");
    let mut edits: Vec<(usize, usize, String)> = of_rule(&document, "type.mismatch")
        .iter()
        .map(|diagnostic| {
            let edit = &diagnostic["next"]["edits"][0];
            (
                edit["start"].as_u64().expect("start") as usize,
                edit["end"].as_u64().expect("end") as usize,
                edit["replacement"]
                    .as_str()
                    .expect("replacement")
                    .to_string(),
            )
        })
        .collect();
    edits.sort();
    for (start, end, replacement) in edits.into_iter().rev() {
        text.replace_range(start..end, &replacement);
    }
    fs::write(&path, text).expect("write");
    let after = json(&jadpo(&root, &["check", "--json"]));
    assert!(
        after["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .all(|diagnostic| diagnostic["severity"] != "error"),
        "{after}"
    );
}

#[test]
fn a_mismatch_with_no_mechanical_edit_still_shows_the_shape() {
    let root = folder("mismatch-address");
    write(
        &root,
        "main.jadpo",
        "type Spot = Object {\n    x: Int\n    y: Int\n}\n\nfunction origin() -> Spot {\n    return \"here\"\n}\n",
    );
    let document = json(&jadpo(&root, &["check", ".", "--json"]));
    let mismatches = of_rule(&document, "type.mismatch");
    let reason = mismatches[0]["reason"].as_str().expect("reason");
    assert!(reason.contains("`Spot { x: Int, y: Int }`"), "{reason}");
    assert!(mismatches[0]["next"].get("edits").is_none(), "{document}");
    assert!(
        !mismatches[0]["next"]["text"]
            .as_str()
            .expect("next")
            .contains("jadpo help")
    );
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?}");
}

#[test]
fn renaming_a_domain_is_the_agents_until_the_project_has_a_commit() {
    for (domain, rule) in [
        ("library", "syntax.expected_name"),
        ("utils", "naming.generic_name"),
    ] {
        let root = folder(&format!("rename-{domain}"));
        write(&root, "app.jadpo", "application {}\n");
        write(
            &root,
            &format!("domains/{domain}/actions/borrow.jadpo"),
            "public action borrow() -> Unit {}\n",
        );
        let decides = |root: &Path| {
            let document = json(&jadpo(root, &["check", "--json"]));
            of_rule(&document, rule)[0]["next"]["decides"]
                .as_str()
                .expect("decides")
                .to_string()
        };
        // Outside git, and in a repository with no commit yet.
        assert_eq!(decides(&root), "agent");
        git(&root, &["init", "-q"]);
        assert_eq!(decides(&root), "agent");
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "first"]);
        assert_eq!(decides(&root), "human");
    }
}

#[test]
fn an_unused_declaration_is_to_be_used_or_removed() {
    let root = folder("unused");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/planning/types/due_window.jadpo",
        "type DueWindow = Int {\n    min: 1\n}\n",
    );
    let document = json(&jadpo(&root, &["check", "--json"]));
    let unused = of_rule(&document, "layout.unused_declaration");
    assert_eq!(
        unused[0]["next"]["text"],
        "Use `DueWindow`, or remove it if nothing will"
    );
}
