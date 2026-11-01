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
