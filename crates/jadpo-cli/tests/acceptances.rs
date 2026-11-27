//! Acceptances (area 18, "Acceptances"; chunk A8), end to end through git:
//! `jadpo check` compares a branch with the default branch at its merge
//! base and fails on each change that needs a person until an acceptance
//! the change adds lists it with a person's `///` reason. Making a route
//! public, removing an API output field, adding an outside host and
//! declining `policy.call_ungranted` each need one; `jadpo acceptance new`
//! drafts it without a reason; an entry matching nothing and a missing
//! reason fail; an acceptance already on the before side is history, and
//! editing one fails.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::support;

pub(crate) fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// [`jadpo`] with `CI` set, as CI runs it: `jadpo check` writes no record
/// or migration and fails on one out of date (area 18).
pub(crate) fn jadpo_ci(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(
        &args,
        &Environment::empty().with_var("CI", "true").with_cwd(cwd),
    );
    output.stdout = support::without_run_time(&output.stdout);
    output
}

pub(crate) fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Jadpo Tests",
            "-c",
            "user.email=tests@jadpo.dev",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(crate) fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
    fs::write(path, text).expect("written");
}

pub(crate) fn edit(root: &Path, path: &str, from: &str, to: &str) {
    let text = fs::read_to_string(root.join(path)).expect("read");
    assert_eq!(text.matches(from).count(), 1, "{from:?} in {path}");
    fs::write(root.join(path), text.replacen(from, to, 1)).expect("written");
}

const BASE: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use lending/entities { Member }

application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}

principal {
    user: {
        subject: Member.handle
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    /// The library's database.
    database_url: PostgresUrl
}
"#,
    ),
    (
        "authentication.jadpo",
        "authentication {\n    api_token: {}\n}\n",
    ),
    (
        "policy.jadpo",
        r#"use lending

policy {
    actions: {
        lending.borrow: [Access.authenticated]
    }
}
"#,
    ),
    (
        "domains/lending/types/handle.jadpo",
        "type Handle = Text {\n    max_length: 40\n}\n",
    ),
    (
        "domains/lending/types/staff_role.jadpo",
        "type StaffRole = Enum {\n    admin\n    clerk\n}\n",
    ),
    (
        "domains/lending/types/book_view.jadpo",
        r#"use lending/entities { Book }

/// A book as members see it.
type BookView = Output {
    pick: [Book.id, Book.title]
}
"#,
    ),
    (
        "domains/lending/entities/member.jadpo",
        r#"use lending/types { Handle }

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
    }
}
"#,
    ),
    (
        "domains/lending/entities/staff.jadpo",
        r#"use lending/entities { Member }
use lending/types { StaffRole }

entity Staff {
    id: Uuid
    member_id: Member.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: member_id
        role: role
    }

    policy: {
        read: [member_id]
    }
}
"#,
    ),
    (
        "domains/lending/entities/book.jadpo",
        r#"use lending/entities { Member }
use lending/types { Handle, StaffRole }

entity Book {
    id: Uuid
    title: Handle
    borrower_id: Member.id? {
        on_delete: OnDelete.restricted
    }

    policy: {
        create: [StaffRole.admin]
        read: [Access.authenticated]
        update: [StaffRole.admin]
    }
}
"#,
    ),
    (
        "domains/lending/queries/catalogue.jadpo",
        r#"use lending/entities { Book }
use lending/types { BookView }

/// Every book, by title.
query catalogue() -> List<BookView> {
    from: Book
    order: [Book.title asc]
    limit: 50
}
"#,
    ),
    (
        "domains/lending/actions/borrow.jadpo",
        r#"use lending/entities { Book, BookNotFound }

/// Borrows a book for the caller.
action borrow(book_id: Book.id, caller: Principal.user) fails BookNotFound -> Unit {
    var book: Book = attempt Book.get(book_id)
    assert book.id == book_id
}
"#,
    ),
    (
        "domains/lending/routes/books.jadpo",
        r#"use lending/actions { borrow }
use lending/entities { Book }
use lending/queries { catalogue }
use lending/types { BookView }

routes Books {
    prefix: /books

    GET / {
        output: List<BookView>
        run: catalogue()
    }

    POST /{book_id: Book.id}/borrow {
        status: SuccessStatus.no_content
        run: borrow(book_id, current_principal.user)
    }
}
"#,
    ),
];

/// The library, committed on `main` with its records and migration, then a
/// branch `change` checked out.
pub(crate) fn library(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("acceptances-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in BASE {
        write(&root, path, text);
    }
    assert_eq!(jadpo(&root, &["fmt"]).code, 0);
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}", migrated.stdout);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}", recorded.stdout);
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "The library"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    git(&root, &["checkout", "-q", "-b", "change"]);
    root
}

/// The rules and messages `jadpo check --json` reports.
fn problems(root: &Path) -> Vec<String> {
    let output = jadpo(root, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|d| {
            format!(
                "{}: {}",
                d["rule"].as_str().unwrap_or(""),
                d["message"].as_str().unwrap_or("")
            )
        })
        .collect()
}

/// The agent's change: the catalogue opened to anyone, a field gone from
/// its output, a second database, and a decline of `policy.call_ungranted`.
pub(crate) fn change(root: &Path) {
    edit(
        root,
        "domains/lending/routes/books.jadpo",
        "    GET / {\n",
        "    GET / {\n        authentication: RouteAuthentication.none\n",
    );
    edit(
        root,
        "domains/lending/entities/book.jadpo",
        "read: [Access.authenticated]",
        "read: [Access.authenticated, Access.public]",
    );
    edit(
        root,
        "domains/lending/types/book_view.jadpo",
        "pick: [Book.id, Book.title]",
        "pick: [Book.id]",
    );
    edit(
        root,
        "app.jadpo",
        "            url: config.database_url\n        }\n",
        "            url: config.database_url\n        }\n        archive: {\n            driver: Driver.postgres\n            url: config.archive_url\n        }\n",
    );
    edit(
        root,
        "config.jadpo",
        "    database_url: PostgresUrl\n",
        "    database_url: PostgresUrl\n    /// Where old loans are kept.\n    archive_url: PostgresUrl\n",
    );
    write(
        root,
        "domains/lending/actions/lend.jadpo",
        r#"use lending/entities { Book, BookNotFound, Member, MemberNotFound }

/// Lends a book to a member.
action lend(book_id: Book.id, member_id: Member.id) fails BookNotFound, MemberNotFound -> Unit {
    var book: Book = attempt Book.get(book_id)
    var lent: Book = attempt Book.update(book) {
        set: {
            borrower_id: member_id
        }
    }
    assert lent.borrower_id == member_id
}
"#,
    );
    write(
        root,
        "domains/lending/actions/borrow.jadpo",
        r#"use lending/actions { lend }
use lending/entities { Book, BookNotFound, MemberNotFound }

/// Borrows a book for the caller.
decline policy.call_ungranted: "Only staff borrow through the desk"
action borrow(book_id: Book.id, caller: Principal.user) fails BookNotFound, MemberNotFound -> Unit {
    attempt lend(book_id, caller.id)
}
"#,
    );
    assert_eq!(jadpo(root, &["fmt"]).code, 0);
    let recorded = jadpo(root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}", recorded.stdout);
}

pub(crate) fn acceptance_files(root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(root.join("acceptances"))
        .map(|entries| entries.filter_map(|e| Some(e.ok()?.path())).collect())
        .unwrap_or_default();
    found.sort();
    found
}

#[test]
fn each_change_that_needs_a_person_fails_until_an_added_acceptance_gives_a_reason() {
    let root = library("needs");
    change(&root);
    assert_eq!(
        problems(&root),
        [
            "acceptance.missing: widening who can do what needs a person: lending.Book.read",
            "acceptance.missing: opening a route to anyone widens who can call it: \"main GET /books\"",
            "acceptance.missing: removing an output field can break clients: main.BookView.title",
            "acceptance.missing: declining a security rule needs a person: lending.borrow",
            "acceptance.missing: a new outside host the app can send data to needs a person: archive",
        ]
    );
    // An agent drafts the acceptance, without a reason, and stops.
    let drafted = jadpo(&root, &["acceptance", "new", "open_the_catalogue"]);
    assert_eq!(drafted.code, 0, "{}", drafted.stdout);
    let files = acceptance_files(&root);
    assert_eq!(files.len(), 1, "{files:?}");
    let name = files[0].file_name().and_then(|n| n.to_str()).unwrap_or("");
    assert!(name.ends_with("_open_the_catalogue.jadpo"), "{name}");
    let text = fs::read_to_string(&files[0]).expect("read");
    assert_eq!(
        text,
        r#"acceptance {
    changes: {
        "main GET /books": PolicyChange.route_public
        archive: NetworkChange.host_added
        lending.Book.read: PolicyChange.widened
        lending.borrow: PolicyChange.call_ungranted_declined
        main.BookView.title: ApiChange.output_field_removed
    }
}
"#
    );
    assert!(
        drafted
            .stdout
            .contains("5 changes wait for a person's reason"),
        "{}",
        drafted.stdout
    );
    let relative = format!("acceptances/{name}");
    assert_eq!(
        problems(&root),
        [format!("acceptance.no_reason: `{relative}` has no reason")]
    );
    // A person writes the reason.
    fs::write(
        &files[0],
        format!("/// The catalogue is public now, as the shop front asked.\n{text}"),
    )
    .expect("written");
    assert_eq!(problems(&root), Vec::<String>::new());
    assert_eq!(jadpo(&root, &["fmt", "--check"]).code, 0);
    // An entry that matches nothing fails.
    edit(
        &root,
        &relative,
        "        main.BookView.title: ApiChange.output_field_removed\n",
        "        main.BookView.title: ApiChange.output_field_removed\n        main.BookView.id: ApiChange.output_field_removed\n",
    );
    assert_eq!(
        problems(&root),
        [
            "acceptance.unmatched: `main.BookView.id: ApiChange.output_field_removed` matches no change"
        ]
    );
}

/// Every error only a person can clear is said once, at the end, with the
/// question to put and the entries their agreement writes, so an agent asks
/// the owner once; each error's own step points there (chunk 10.9a: agents
/// spent 11k tokens a session in `acceptance.missing` loops).
#[test]
fn every_sign_off_is_asked_once_with_its_entries() {
    let root = library("sign-off");
    change(&root);
    let output = jadpo(&root, &["check"]);
    assert_eq!(output.code, 1, "{}", output.stdout);
    assert_eq!(
        output
            .stdout
            .matches("next: Ask your user, with the other questions below (human)")
            .count(),
        5,
        "{}",
        output.stdout
    );
    let block = output
        .stdout
        .split("decision: 5 errors only your user can clear\n")
        .nth(1)
        .expect("one sign-off block");
    assert!(
        block.starts_with(
            "  ask: Widening who can do what needs a person: lending.Book.read. Who may do it, and to which records?\n"
        ),
        "{block}"
    );
    assert!(
        block.contains(
            "  accept:\n    lending.Book.read: PolicyChange.widened\n    \"main GET /books\": PolicyChange.route_public\n"
        ),
        "{block}"
    );
    assert!(
        block.contains("(human)\n\ncheck failed: 5 errors"),
        "{block}"
    );
    // `--json` gives the same, and the steps stay whole.
    let json = jadpo(&root, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(
        document["data"]["sign_off"][2]["entries"],
        serde_json::json!(["main.BookView.title: ApiChange.output_field_removed"])
    );
    assert!(
        document["diagnostics"][0]["next"]["text"]
            .as_str()
            .unwrap_or("")
            .starts_with("Stop and ask your user")
    );
}

/// Each diagnostic's rule and `next.text`, as `jadpo check --json` gives
/// them.
fn next_steps(root: &Path) -> Vec<(String, String)> {
    let output = jadpo(root, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|d| {
            (
                d["rule"].as_str().unwrap_or("").to_string(),
                d["next"]["text"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

/// An acceptance is the owner's decision, so its errors tell an agent to
/// stop and ask them, not to write one: a held-out run drafted the
/// acceptance itself and stopped with `check` red without asking (chunk
/// 10.4c).
#[test]
fn an_acceptance_error_tells_an_agent_to_stop_and_ask() {
    let root = library("ask");
    change(&root);
    let asked = "Stop and ask your user whether to accept this, saying what it allows and for whom: the decision is theirs, and a general go-ahead isn't one. Once they agree to this change, `jadpo acceptance new <purpose>` writes it, and its reason quotes their words";
    for (rule, next) in next_steps(&root) {
        assert_eq!(rule, "acceptance.missing");
        assert_eq!(next, asked);
    }
    assert_eq!(
        jadpo(&root, &["acceptance", "new", "open_the_catalogue"]).code,
        0
    );
    assert_eq!(
        next_steps(&root),
        [(
            "acceptance.no_reason".to_string(),
            "Write your user's own words agreeing to these changes as the reason, with who said them and where, and never a reason of your own; if they haven't agreed, stop and ask them, and if they don't accept, delete the draft and undo the changes".to_string()
        )]
    );
}

#[test]
fn an_acceptance_on_the_before_side_is_history() {
    let root = library("history");
    edit(
        &root,
        "domains/lending/types/book_view.jadpo",
        "pick: [Book.id, Book.title]",
        "pick: [Book.id]",
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    assert_eq!(jadpo(&root, &["acceptance", "new", "drop_titles"]).code, 0);
    let files = acceptance_files(&root);
    let relative = format!(
        "acceptances/{}",
        files[0].file_name().and_then(|n| n.to_str()).unwrap_or("")
    );
    let text = fs::read_to_string(&files[0]).expect("read");
    fs::write(
        &files[0],
        format!("/// Titles move to the details page.\n{text}"),
    )
    .expect("written");
    assert_eq!(problems(&root), Vec::<String>::new());
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "Drop titles"]);
    git(&root, &["checkout", "-q", "main"]);
    git(&root, &["merge", "-q", "--ff-only", "change"]);
    // On the default branch, the change is compared with the commit before.
    assert_eq!(problems(&root), Vec::<String>::new());
    // On the next branch it is history, and editing it is an error.
    git(&root, &["checkout", "-q", "-b", "next"]);
    assert_eq!(problems(&root), Vec::<String>::new());
    edit(&root, &relative, "Titles move", "Titles moved");
    assert_eq!(
        problems(&root),
        [format!(
            "acceptance.edited: `{relative}` was accepted before this change"
        )]
    );
}

/// Who may set a field is policy (area 9, "Who may set a field"; chunk
/// 10.4b): letting callers set one only the store set, or change one that
/// never changed, needs a person, and so does naming who may change a
/// field whose own `update:` named nobody.
#[test]
fn letting_callers_set_a_field_they_could_not_needs_a_person() {
    let root = library("set-by");
    git(&root, &["checkout", "-q", "main"]);
    write(
        &root,
        "domains/lending/entities/plaque.jadpo",
        r#"use lending/entities { Book }
use lending/types { Handle, StaffRole }

/// A plaque on a donated book.
entity Plaque {
    id: Uuid
    book_id: Book.id {
        on_delete: OnDelete.cascade
    }
    shelf_mark: Handle {
        immutable: true
    }
    note: Handle? {
        policy: {
            update: []
        }
    }
    added_at: Instant {
        generated: Generated.on_create
    }

    policy: {
        create: [StaffRole.admin]
        read: [Access.authenticated]
        update: [StaffRole.admin]
    }
}
"#,
    );
    let migrated = jadpo(&root, &["migrate", "new", "add_plaques"]);
    assert_eq!(migrated.code, 0, "{}", migrated.stdout);
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let access = fs::read_to_string(root.join("access.jadpo_record")).expect("read");
    assert!(
        access.contains(
            "field lending.Plaque.added_at\n    generated\n\nfield lending.Plaque.note\n    update nobody\n\nfield lending.Plaque.shelf_mark\n    immutable\n"
        ),
        "{access}"
    );
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "Plaques"]);
    git(&root, &["checkout", "-q", "-b", "set-by"]);
    assert_eq!(problems(&root), Vec::<String>::new());
    let plaque = "domains/lending/entities/plaque.jadpo";
    edit(
        &root,
        plaque,
        "    shelf_mark: Handle {\n        immutable: true\n    }\n",
        "    shelf_mark: Handle\n",
    );
    edit(
        &root,
        plaque,
        "            update: []\n",
        "            update: [StaffRole.admin]\n",
    );
    edit(
        &root,
        plaque,
        "    added_at: Instant {\n        generated: Generated.on_create\n    }\n",
        "    added_at: Instant\n",
    );
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}", recorded.stdout);
    assert_eq!(
        problems(&root),
        [
            "acceptance.missing: removing a field's restriction widens who can reach it: lending.Plaque.added_at, lending.Plaque.shelf_mark",
            "acceptance.missing: widening who can do what needs a person: lending.Plaque.note.update",
        ]
    );
}

#[test]
fn with_nothing_to_compare_with_check_says_so_in_one_line() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("acceptances-nothing");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in BASE {
        write(&root, path, text);
    }
    assert_eq!(jadpo(&root, &["fmt"]).code, 0);
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    git(&root, &["init", "-q"]);
    // No commit yet: nothing to compare with.
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    assert!(
        checked
            .stdout
            .contains("before: none, so acceptances aren't judged: no commit yet"),
        "{}",
        checked.stdout
    );
    let drafted = jadpo(&root, &["acceptance", "new", "anything"]);
    assert_eq!(drafted.code, 2, "{}", drafted.stdout);
    let said = format!("{}{}", drafted.stdout, drafted.stderr);
    assert!(
        said.contains("Nothing to compare with: no commit yet"),
        "{said}"
    );
}

/// A clone made without the base commit, as CI's depth-1 checkout is, is
/// an error naming `git fetch`, rather than nothing compared.
#[test]
fn a_clone_without_the_base_commit_says_to_fetch_it() {
    let root = library("shallow-origin");
    edit(
        &root,
        "domains/lending/types/book_view.jadpo",
        "pick: [Book.id, Book.title]",
        "pick: [Book.id]",
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "Drop titles"]);
    let clone = Path::new(env!("CARGO_TARGET_TMPDIR")).join("acceptances-shallow");
    let _ = fs::remove_dir_all(&clone);
    let source = format!("file://{}", root.display());
    git(
        Path::new(env!("CARGO_TARGET_TMPDIR")),
        &[
            "clone",
            "-q",
            "--depth",
            "1",
            "--single-branch",
            "--branch",
            "change",
            &source,
            "acceptances-shallow",
        ],
    );
    assert_eq!(
        problems(&clone),
        [
            "acceptance.base_missing: The base to compare with, the default branch, isn't in this clone"
        ]
    );
}

/// Copies the teaching example `name` into a repository of its own,
/// committed on `main`, with a branch `change` checked out.
pub(crate) fn example(name: &str) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("acceptances-{name}"));
    let _ = fs::remove_dir_all(&root);
    copy(&from, &root);
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "The example"]);
    git(&root, &["checkout", "-q", "-b", "change"]);
    root
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("folder");
    for entry in fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if entry.file_name() == "build" {
            continue;
        }
        if path.is_dir() {
            copy(&path, &to.join(entry.file_name()));
        } else {
            fs::copy(&path, to.join(entry.file_name())).expect("copied");
        }
    }
}

/// A sign-in setting loosened past its limit (area 8, "Settings") needs a
/// person; one loosened within it doesn't.
#[test]
fn a_sign_in_setting_loosened_past_its_limit_needs_a_person() {
    let root = example("27-guest-house");
    assert_eq!(problems(&root), Vec::<String>::new());
    edit(
        &root,
        "authentication.jadpo",
        "magic_link: {\n",
        "magic_link: {\n            expires_after: 1h\n",
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    assert_eq!(problems(&root), Vec::<String>::new());
    edit(
        &root,
        "authentication.jadpo",
        "expires_after: 1h",
        "expires_after: 2h",
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    assert_eq!(
        problems(&root),
        [
            "acceptance.missing: loosening sign-in past its limit weakens it: magic_link.expires_after"
        ]
    );
}

/// An app with a `connections:` block keeps `network.jadpo_record` even
/// while it reaches no host, so its first outside host needs a person like
/// any other, rather than arriving with a record compared with nothing.
#[test]
fn an_apps_first_outside_host_needs_a_person() {
    let root = example("24-seed-library");
    assert_eq!(problems(&root), Vec::<String>::new());
    edit(
        &root,
        "app.jadpo",
        "    connections: {\n",
        "    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    /// The library's database.\n    database_url: PostgresUrl\n}\n",
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    assert_eq!(
        problems(&root),
        ["acceptance.missing: a new outside host the app can send data to needs a person: main"]
    );
}

/// A project whose base commit had no `connections:` block reached no
/// outside host, so it has no network record there; that is read as an
/// empty one, and the change that adds the block and its first host needs
/// a person, until an acceptance with a reason lists it.
#[test]
fn a_project_with_no_connections_gaining_its_first_host_needs_a_person() {
    let parent = Path::new(env!("CARGO_TARGET_TMPDIR")).join("acceptances-first-connection");
    let _ = fs::remove_dir_all(&parent);
    fs::create_dir_all(&parent).expect("folder");
    assert_eq!(jadpo(&parent, &["new", "desk"]).code, 0);
    let root = parent.join("desk");
    assert!(!root.join("network.jadpo_record").exists());
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "The desk"]);
    git(&root, &["checkout", "-q", "-b", "change"]);
    assert_eq!(problems(&root), Vec::<String>::new());
    edit(
        &root,
        "app.jadpo",
        "application {\n",
        "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n    }\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    /// The desk's database.\n    database_url: PostgresUrl\n}\n",
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    assert!(root.join("network.jadpo_record").exists());
    assert_eq!(
        problems(&root),
        ["acceptance.missing: a new outside host the app can send data to needs a person: main"]
    );
    // Compared, so the change isn't one of those `check` says it can't judge.
    let checked = jadpo(&root, &["check"]);
    assert!(!checked.stdout.contains("before:"), "{}", checked.stdout);
    assert_eq!(
        jadpo(&root, &["acceptance", "new", "add_a_database"]).code,
        0
    );
    let files = acceptance_files(&root);
    let text = fs::read_to_string(&files[0]).expect("read");
    assert_eq!(
        text,
        "acceptance {\n    changes: {\n        main: NetworkChange.host_added\n    }\n}\n"
    );
    fs::write(
        &files[0],
        format!("/// The desk keeps its loans in PostgreSQL from now on.\n{text}"),
    )
    .expect("written");
    assert_eq!(problems(&root), Vec::<String>::new());
}

/// A change that widens a grant and changes the schema needs both a
/// person's acceptance and a migration, and neither waits for the other: a
/// held-out run (chunk 10.7d) was told to fix the acceptance first by the
/// migration's error, and `jadpo acceptance new` stopped on the migration.
#[test]
fn a_migration_and_an_acceptance_never_wait_for_each_other() {
    let root = library("both");
    let widen = |root: &Path| {
        edit(
            root,
            "domains/lending/entities/book.jadpo",
            "read: [Access.authenticated]",
            "read: [Access.authenticated, Access.public]",
        );
        edit(
            root,
            "domains/lending/entities/book.jadpo",
            "    title: Handle\n",
            "    title: Handle\n    shelf: Handle?\n",
        );
    };
    widen(&root);
    // In CI nothing is written, and the acceptance is drafted all the same.
    let ci: Vec<String> = {
        let output = jadpo_ci(&root, &["check", "--json"]);
        let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
        document["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .map(|d| d["rule"].as_str().unwrap_or("").to_string())
            .collect()
    };
    assert_eq!(
        ci,
        [
            "acceptance.missing",
            "record.out_of_date",
            "migration.out_of_date"
        ]
    );
    let drafted = jadpo(&root, &["acceptance", "new", "open_the_catalogue"]);
    assert_eq!(drafted.code, 0, "{}", drafted.stdout);
    assert_eq!(acceptance_files(&root).len(), 1);
    // Outside CI, `jadpo check` writes the migration itself beside the
    // acceptance waiting for its reason.
    assert_eq!(
        next_steps(&root)
            .into_iter()
            .map(|(rule, _)| rule)
            .collect::<Vec<_>>(),
        ["acceptance.no_reason"]
    );
    let migrations = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .count();
    assert_eq!(migrations, 2);

    // And before any acceptance: the migration is written beside the
    // error waiting for a person, which is all that is left.
    let root = library("both-first");
    widen(&root);
    assert_eq!(
        next_steps(&root)
            .into_iter()
            .map(|(rule, _)| rule)
            .collect::<Vec<_>>(),
        ["acceptance.missing"]
    );
}

/// When what is left beside a migration needing a decision waits for a
/// person, the migration's `next:` names its own command, not "fix the
/// other errors first" (chunk 10.7d).
#[test]
fn a_migration_waiting_for_a_decision_beside_a_persons_error_names_its_command() {
    let root = library("decision");
    edit(
        &root,
        "domains/lending/entities/book.jadpo",
        "read: [Access.authenticated]",
        "read: [Access.authenticated, Access.public]",
    );
    edit(
        &root,
        "domains/lending/entities/book.jadpo",
        "    title: Handle\n",
        "    title: Handle\n    shelf: Handle\n",
    );
    let steps = next_steps(&root);
    let migration: Vec<&String> = steps
        .iter()
        .filter(|(rule, _)| rule == "migration.out_of_date")
        .map(|(_, next)| next)
        .collect();
    assert_eq!(migration.len(), 1, "{steps:?}");
    assert!(
        migration[0].starts_with("Run `jadpo migrate new "),
        "{steps:?}"
    );
}

/// A desk whose saved answers only staff read, and whose answers every
/// member reads (chunk 10.7c).
const DESK: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use desk/entities { User }

application {}

principal {
    user: {
        subject: User.handle
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    api_token: {}
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use desk/entities { Workspace }

policy {
    tenant: Workspace
}
"#,
    ),
    (
        "domains/desk/types/role.jadpo",
        r#"/// What a member may do.
type Role = Enum {
    owner
    agent
    viewer
}
"#,
    ),
    (
        "domains/desk/types/body.jadpo",
        r#"/// A message's text.
type Body = Text {
    max_length: 500
}
"#,
    ),
    (
        "domains/desk/types/saved_view.jadpo",
        r#"use desk/entities { SavedAnswer }

/// A saved answer.
type SavedView = Output {
    pick: [SavedAnswer.id, SavedAnswer.body]
}
"#,
    ),
    (
        "domains/desk/entities/user.jadpo",
        r#"use desk/types { Body }

/// Someone who signs in.
entity User {
    id: Uuid
    handle: Body {
        unique: true
    }

    policy: {
        read: [id]
        tenant: Tenancy.shared
    }
}
"#,
    ),
    (
        "domains/desk/entities/workspace.jadpo",
        r#"use desk/types { Body, Role }

/// The tenant.
entity Workspace {
    id: Uuid
    name: Body

    policy: {
        read: [Role.owner, Role.agent, Role.viewer]
    }
}
"#,
    ),
    (
        "domains/desk/entities/membership.jadpo",
        r#"use desk/entities { User, Workspace }
use desk/types { Role }

/// A role in a workspace.
entity Membership {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: Role

    membership: {
        scope: workspace_id
        member: user_id
        role: role
    }

    policy: {
        read: [Role.owner, user_id]
    }
}
"#,
    ),
    (
        "domains/desk/entities/saved_answer.jadpo",
        r#"use desk/entities { Workspace }
use desk/types { Body, Role }

/// A saved answer staff reuse.
entity SavedAnswer {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    body: Body

    policy: {
        create: [Role.owner]
        read: [Role.owner, Role.agent]
    }
}
"#,
    ),
    (
        "domains/desk/entities/answer.jadpo",
        r#"use desk/entities { Workspace }
use desk/types { Body, Role }

/// An answer every member reads.
entity Answer {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    body: Body

    policy: {
        create: [Role.owner, Role.agent]
        read: [Role.owner, Role.agent, Role.viewer]
    }
}
"#,
    ),
    (
        "domains/desk/queries/saved_by_id.jadpo",
        r#"use desk/entities { SavedAnswer, SavedAnswerNotFound }
use desk/types { SavedView }

/// One saved answer.
query saved_by_id(saved_id: SavedAnswer.id) fails SavedAnswerNotFound -> SavedView {
    from: SavedAnswer
    where: SavedAnswer.id == saved_id
    missing: SavedAnswerNotFound
}
"#,
    ),
];

const ANSWER_FROM_SAVED: &str = r#"use desk/entities { Answer, SavedAnswer, SavedAnswerNotFound, Workspace, WorkspaceNotFound }

/// Answers with a saved answer's text.
action answer_from_saved(workspace_id: Workspace.id, saved_id: SavedAnswer.id) fails SavedAnswerNotFound, WorkspaceNotFound -> Answer {
    var saved: SavedAnswer = attempt SavedAnswer.get(saved_id)
    return attempt Answer.create {
        set: {
            workspace_id: workspace_id
            body: saved.body
        }
    }
}
"#;

/// A field copied into one more people read is refused until an
/// acceptance names that field flow; the acceptance that adds it matches
/// the `copy` line it adds to the access record, and it goes on allowing
/// the copy, from any action, on the branches after it (chunk 10.7c).
#[test]
fn copying_a_restricted_field_needs_an_acceptance_naming_the_flow() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("acceptances-copy");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in DESK {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}", migrated.stdout);
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "The desk"]);
    git(&root, &["checkout", "-q", "-b", "copy"]);
    let action = "domains/desk/actions/answer_from_saved.jadpo";
    write(&root, action, ANSWER_FROM_SAVED);
    assert_eq!(
        problems(&root),
        [
            "policy.output_field_unproved: `answer_from_saved` stores `SavedAnswer.body` in `Answer.body`, which not everyone who may read `body` may read"
        ]
    );
    let accepted = "acceptances/20261125T1000_send_saved_answers.jadpo";
    write(
        &root,
        accepted,
        "/// Sam Okafor (support lead), in chat on 2026-11-25: \"Viewers may see a saved answer's text once it is sent as an answer.\"\nacceptance {\n    changes: {\n        \"desk.SavedAnswer.body -> desk.Answer.body\": PolicyChange.field_copied\n    }\n}\n",
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let access = fs::read_to_string(root.join("access.jadpo_record")).expect("read");
    assert!(
        access.contains("\ncopy desk.SavedAnswer.body -> desk.Answer.body\n"),
        "{access}"
    );
    assert_eq!(problems(&root), Vec::<String>::new());
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "Send saved answers"]);
    git(&root, &["checkout", "-q", "main"]);
    git(&root, &["merge", "-q", "--ff-only", "copy"]);
    git(&root, &["checkout", "-q", "-b", "again"]);
    // A second action making the same copy needs nothing more.
    write(
        &root,
        "domains/desk/actions/answer_again.jadpo",
        &ANSWER_FROM_SAVED.replace("answer_from_saved", "answer_again"),
    );
    assert_eq!(problems(&root), Vec::<String>::new());
}

/// The reason each `acceptance.unmatched` gives, by its message.
fn unmatched_reasons(root: &Path) -> Vec<String> {
    let output = jadpo(root, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter(|d| d["rule"] == "acceptance.unmatched")
        .map(|d| d["reason"].as_str().unwrap_or("").to_string())
        .collect()
}

/// Chunk 10.13: an entry matching nothing says which baseline the change
/// was compared with and why, and, while other errors stand, which records
/// weren't compared, so the entry may be right after all.
#[test]
fn an_unmatched_entry_says_what_it_was_compared_with() {
    let root = library("unmatched-baseline");
    let base = Command::new("git")
        .args(["rev-parse", "--short=7", "main"])
        .current_dir(&root)
        .output()
        .expect("git runs");
    let base = String::from_utf8_lossy(&base.stdout).trim().to_string();
    write(
        &root,
        "acceptances/20261127T0900_remove_title.jadpo",
        "/// Nobody reads the title, as the shop front said.\nacceptance {\n    changes: {\n        main.BookView.title: ApiChange.output_field_removed\n    }\n}\n",
    );
    // With no remote, the default branch is the local `main`.
    let reasons = unmatched_reasons(&root);
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(
        reasons[0].ends_with(&format!(
            "This change was compared with main at {base}, the local branch: no remote names a default branch."
        )),
        "{}",
        reasons[0]
    );
    // The change is made, but a route's error keeps the API records from
    // being compared: the entry is unmatched only for now.
    edit(
        &root,
        "domains/lending/types/book_view.jadpo",
        "pick: [Book.id, Book.title]",
        "pick: [Book.id]",
    );
    edit(
        &root,
        "domains/lending/routes/books.jadpo",
        "    GET / {\n",
        "    GET / {\n        authentication: RouteAuthentication.nobody\n",
    );
    let reasons = unmatched_reasons(&root);
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(
        reasons[0].contains(
            "While other errors stand, the API records aren't compared, so a change in them isn't seen yet."
        ),
        "{}",
        reasons[0]
    );
}
