//! The approval report (area 17, "Reviewing a change"; area 18,
//! "Comparing two builds"; chunk V2), end to end through git: `jadpo
//! approval` builds the before side, the default branch at its merge
//! base, caches its manifest in `build/before/<commit>/`, and writes
//! `build/review/approval.md` and `.json` in the page's order: each
//! decision that needs a person in plain words, with whether its
//! acceptance exists and the path that makes it true; the declarations
//! newly reachable; the other behaviour; and the source by declaration.

use std::fs;
use std::path::Path;

use crate::acceptances::{acceptance_files, change, edit, example, git, jadpo, library, write};

/// The agent's change, as `acceptances` makes it, with a second route
/// running the catalogue query, which doesn't change but is now reached
/// from somewhere new.
fn changed(name: &str) -> std::path::PathBuf {
    let root = library(name);
    change(&root);
    edit(
        &root,
        "domains/lending/routes/books.jadpo",
        "    POST /{book_id: Book.id}/borrow {\n",
        "    GET /shelf {\n        output: List<BookView>\n        run: catalogue()\n    }\n\n    POST /{book_id: Book.id}/borrow {\n",
    );
    assert_eq!(jadpo(&root, &["fmt"]).code, 0);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}", recorded.stdout);
    root
}

fn report(root: &Path) -> serde_json::Value {
    let output = jadpo(root, &["approval", "--json"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    document["data"].clone()
}

fn strings(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .as_array()
        .expect("a list")
        .iter()
        .map(|item| item[key].as_str().unwrap_or("").to_string())
        .collect()
}

/// A path as its steps' names.
fn steps(path: &serde_json::Value) -> Vec<String> {
    strings(path, "name")
}

#[test]
fn each_decision_is_said_in_plain_words_with_the_path_that_makes_it_true() {
    let root = changed("approval-decisions");
    let data = report(&root);
    let decisions = &data["decisions"];
    assert_eq!(
        strings(decisions, "said"),
        [
            "anyone can now call `GET /books`, without signing in",
            "connection `archive` can now reach `config.archive_url`",
            "anyone can now read `lending.Book` records",
            "callers no grant names can now call `lending.borrow`, which declines `policy.call_ungranted`",
            "clients no longer get `main.BookView.title`",
        ]
    );
    assert_eq!(
        strings(decisions, "subject"),
        [
            "\"main GET /books\"",
            "archive",
            "lending.Book.read",
            "lending.borrow",
            "main.BookView.title",
        ]
    );
    assert_eq!(
        strings(decisions, "acceptance"),
        ["missing", "missing", "missing", "missing", "missing"]
    );
    let first_path = |i: usize| steps(&decisions[i]["paths"][0]);
    assert_eq!(
        first_path(0),
        [
            "anyone",
            "GET /books",
            "lending.catalogue",
            "List<BookView>"
        ]
    );
    // Nothing calls through the new connection yet.
    assert_eq!(decisions[1]["paths"], serde_json::json!([]));
    // Anyone reaches the records only through the routes anyone may call.
    assert_eq!(
        decisions[2]["paths"].as_array().map(Vec::len),
        Some(1),
        "{}",
        decisions[2]
    );
    assert_eq!(
        first_path(2),
        [
            "anyone",
            "GET /books",
            "lending.catalogue",
            "List<BookView>"
        ]
    );
    assert_eq!(
        first_path(3),
        ["POST /books/{book_id}/borrow", "lending.borrow"]
    );
    assert_eq!(first_path(4), ["a client", "GET /books", "BookView.title"]);
    assert_eq!(
        steps(&decisions[4]["paths"][1]),
        ["a client", "GET /books/shelf", "BookView.title"]
    );
    // Each step that is a declaration says where it is.
    assert_eq!(
        decisions[0]["paths"][0][1]["file"],
        "domains/lending/routes/books.jadpo"
    );
    assert_eq!(decisions[2]["record"]["file"], "access.jadpo_record");

    // The query didn't change, and a new route reaches it.
    let reachable = &data["newly_reachable"];
    assert_eq!(strings(reachable, "name"), ["lending.catalogue"]);
    assert_eq!(
        reachable[0]["from"],
        serde_json::json!(["GET /books/shelf"])
    );

    // The other behaviour, from the records and the two manifests.
    let other = &data["other"];
    assert_eq!(
        strings(&other["access"], "heading"),
        [
            "entity lending.Book",
            "decline policy.call_ungranted lending.borrow"
        ]
    );
    assert!(
        strings(&other["integrations"], "heading").contains(&"connection archive".to_string()),
        "{}",
        other["integrations"]
    );
    assert!(
        strings(&other["api"], "heading").contains(&"GET /books/shelf".to_string()),
        "{}",
        other["api"]
    );
    assert!(
        strings(&other["failures"], "said")
            .contains(&"`lending.borrow` can now fail with `MemberNotFound`".to_string()),
        "{}",
        other["failures"]
    );

    // The source, by declaration: what changed, and where.
    let source = &data["source"];
    let names: Vec<String> = source
        .as_array()
        .expect("a list")
        .iter()
        .map(|item| {
            format!(
                "{} {} {}",
                item["change"].as_str().unwrap_or(""),
                item["kind"].as_str().unwrap_or(""),
                item["name"].as_str().unwrap_or("")
            )
        })
        .collect();
    for wanted in [
        "added action lend",
        "changed action borrow",
        "changed entity Book",
        "changed type BookView",
        "added route GET /books/shelf",
        "added connection archive",
    ] {
        assert!(names.contains(&wanted.to_string()), "{wanted}: {names:?}");
    }
    // A failure the compiler provides for an entity is the entity's own
    // source, so it isn't listed twice.
    assert!(
        !names.iter().any(|name| name.ends_with("BookNotFound")),
        "{names:?}"
    );
}

/// The path follows a field to the output that carries it, and only
/// those a query's line lets in can take the way through it.
#[test]
fn a_path_ends_at_the_output_field_and_respects_a_querys_line() {
    let root = example("29-lending-desk");
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    // Who gave a book, for the front desk too: its field and the query
    // that shows it.
    edit(
        &root,
        "domains/desk/entities/book.jadpo",
        "    donor: DisplayName? {\n        policy: {\n            read: [StaffRole.admin]",
        "    donor: DisplayName? {\n        policy: {\n            read: [StaffRole.admin, StaffRole.front_desk]",
    );
    edit(
        &root,
        "policy.jadpo",
        "desk.donations: [StaffRole.admin]",
        "desk.donations: [StaffRole.admin, StaffRole.front_desk]",
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let data = report(&root);
    let decisions = &data["decisions"];
    assert_eq!(
        strings(decisions, "said"),
        [
            "`StaffRole.front_desk` can now read `desk.Book.donor`",
            "`StaffRole.front_desk` can now run `desk.donations`",
        ]
    );
    assert_eq!(
        steps(&decisions[0]["paths"][0]),
        [
            "StaffRole.front_desk",
            "GET /staff/donations",
            "desk.donations",
            "DonatedBook.donor"
        ]
    );
    assert_eq!(
        steps(&decisions[1]["paths"][0]),
        [
            "StaffRole.front_desk",
            "GET /staff/donations",
            "desk.donations"
        ]
    );
    // Where it is shelved, for everyone signed in: the only query that
    // shows it is still listed for the staff, so no way reaches it yet.
    git(&root, &["checkout", "-q", "."]);
    edit(
        &root,
        "domains/desk/entities/book.jadpo",
        "read: [StaffRole.admin, StaffRole.front_desk]",
        "read: [StaffRole.admin, StaffRole.front_desk, Access.authenticated]",
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let data = report(&root);
    assert_eq!(
        strings(&data["decisions"], "said"),
        ["anyone signed in can now read `desk.Book.shelf`"]
    );
    assert_eq!(data["decisions"][0]["paths"], serde_json::json!([]));
    let markdown = fs::read_to_string(root.join("build/review/approval.md")).expect("written");
    assert!(
        markdown.contains("- **Path:** no route, schedule or subscriber reaches it yet."),
        "{markdown}"
    );
}

#[test]
fn the_markdown_report_follows_the_pages_order() {
    let root = changed("approval-markdown");
    let output = jadpo(&root, &["approval"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    let markdown = fs::read_to_string(root.join("build/review/approval.md")).expect("written");
    let json = fs::read_to_string(root.join("build/review/approval.json")).expect("written");
    assert!(json.contains("\"decisions\""), "{json}");
    let order = [
        "# Approval",
        "## Decisions that need a person",
        "### Anyone can now call `GET /books`, without signing in",
        "anyone → `GET /books` → `lending.catalogue` → `List<BookView>`",
        "## Newly reachable code",
        "## Other behaviour",
        "### The API record",
        "### Schema and migrations",
        "### Failures",
        "### Events and subscribers",
        "### Integrations",
        "### Who can do what",
        "### Declines",
        "### Prompts",
        "### Wording not yet accepted",
        "## Source by declaration",
    ];
    let mut at = 0;
    for heading in order {
        let Some(found) = markdown[at..].find(heading) else {
            panic!("`{heading}` after byte {at}:\n{markdown}");
        };
        at += found + heading.len();
    }
    assert!(
        markdown.contains("```diff\n  entity lending.Book\n+     read Access.public\n"),
        "{markdown}"
    );
    // Every decline the change adds, with its reason.
    assert!(
        markdown.contains(
            "- `lending.borrow` declines `policy.call_ungranted`: Only staff borrow through the desk, `domains/lending/actions/borrow.jadpo:5`"
        ),
        "{markdown}"
    );
    // The terminal says the decisions in full and counts the rest.
    let screen = &output.stdout;
    assert!(
        screen.starts_with("5 decisions need a person\n"),
        "{screen}"
    );
    assert!(
        screen.contains(
            "  anyone can now read `lending.Book` records (acceptance missing)\n    anyone → GET /books → lending.catalogue → List<BookView>\n"
        ),
        "{screen}"
    );
    assert!(
        screen.contains("1 declaration newly reachable\n"),
        "{screen}"
    );
    assert!(screen.contains("  before: main at "), "{screen}");
    assert!(
        screen.contains("Wrote build/review/approval.md\n"),
        "{screen}"
    );
    assert!(
        screen.contains("decision: 5 changes wait for a person's acceptance\n"),
        "{screen}"
    );
}

/// Access also widens when an output gains a field or an action starts
/// touching another entity, with no grant changed (area 9, "Protecting
/// policy"): the report shows those ways there too.
#[test]
fn what_widens_access_without_changing_a_grant_shows_too() {
    let root = library("approval-reach");
    edit(
        &root,
        "domains/lending/types/book_view.jadpo",
        "pick: [Book.id, Book.title]",
        "pick: [Book.id, Book.title, Book.borrower_id]",
    );
    edit(
        &root,
        "domains/lending/actions/borrow.jadpo",
        "use lending/entities { Book, BookNotFound }",
        "use lending/entities { Book, BookNotFound, Member, MemberNotFound }",
    );
    edit(
        &root,
        "domains/lending/actions/borrow.jadpo",
        "fails BookNotFound -> Unit {\n",
        "fails BookNotFound, MemberNotFound -> Unit {\n    var member: Member = attempt Member.get(caller.id)\n    assert member.id == caller.id\n",
    );
    assert_eq!(jadpo(&root, &["fmt"]).code, 0);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}", recorded.stdout);
    let data = report(&root);
    assert_eq!(
        strings(&data["other"]["access"], "said"),
        [
            "`lending.borrow` now reads `Member` records",
            "`BookView` now carries `Book.borrower_id`",
        ]
    );
}

/// With `review: TranslationReview.required`, wording not yet accepted is
/// listed under other behaviour; an app that doesn't review says so.
#[test]
fn wording_not_yet_accepted_is_listed_under_other_behaviour() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("approval-wording");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in crate::translation_tooling::SHOP {
        write(&root, path, text);
    }
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}", recorded.stdout);
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "The shop"]);
    git(&root, &["checkout", "-q", "-b", "change"]);
    let data = report(&root);
    let wording = strings(&data["other"]["wording"], "said");
    assert!(
        wording.contains(
            &"`order_shipped` in `de_de`, in `domains/shop/translations/de_de.jadpo`: new"
                .to_string()
        ),
        "{wording:?}"
    );
    let screen = jadpo(&root, &["approval"]).stdout;
    assert!(
        screen.contains("  wording: 7 translations not yet accepted\n"),
        "{screen}"
    );
    let library = library("approval-no-review");
    assert_eq!(report(&library)["other"]["wording"], serde_json::json!([]));
    let markdown = fs::read_to_string(library.join("build/review/approval.md")).expect("written");
    assert!(
        markdown.contains(
            "Not reviewed: the app doesn't set `translations: { review: TranslationReview.required }`."
        ),
        "{markdown}"
    );
}

#[test]
fn each_decision_says_whether_its_acceptance_exists() {
    let root = changed("approval-acceptances");
    assert_eq!(
        jadpo(&root, &["acceptance", "new", "open_the_catalogue"]).code,
        0
    );
    let files = acceptance_files(&root);
    let relative = format!(
        "acceptances/{}",
        files[0].file_name().and_then(|n| n.to_str()).unwrap_or("")
    );
    let data = report(&root);
    assert_eq!(data["decisions"][0]["acceptance"], "reason missing");
    assert_eq!(data["decisions"][0]["file"], relative.as_str());
    let screen = jadpo(&root, &["approval"]).stdout;
    assert!(
        screen.contains("decision: 5 changes wait for a person's reason\n"),
        "{screen}"
    );
    let text = fs::read_to_string(&files[0]).expect("read");
    fs::write(
        &files[0],
        format!("/// The catalogue is public now, as the shop front asked.\n{text}"),
    )
    .expect("written");
    let data = report(&root);
    assert_eq!(
        strings(&data["decisions"], "acceptance"),
        ["accepted", "accepted", "accepted", "accepted", "accepted"]
    );
    assert_eq!(
        data["decisions"][0]["reason"],
        "The catalogue is public now, as the shop front asked."
    );
    let markdown = fs::read_to_string(root.join("build/review/approval.md")).expect("written");
    assert!(
        markdown.contains("The catalogue is public now, as the shop front asked."),
        "{markdown}"
    );
    let screen = jadpo(&root, &["approval"]).stdout;
    assert!(!screen.contains("decision:"), "{screen}");
}

#[test]
fn the_base_is_built_once_and_its_manifest_kept() {
    let root = changed("approval-cache");
    let first = report(&root);
    let commit = first["before"]["commit"]
        .as_str()
        .expect("a commit")
        .to_string();
    assert_eq!(commit.len(), 40, "{commit}");
    let cached = root.join(format!("build/before/{commit}/manifest.json"));
    let manifest = fs::read_to_string(&cached).expect("cached");
    assert!(manifest.contains("\"route GET /books\""), "{manifest}");
    // The kept manifest is what the next report compares with.
    assert_eq!(report(&root), first);
}

#[test]
fn before_names_the_branch_or_commit_to_compare_with() {
    let root = changed("approval-before");
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "The change"]);
    // Compared with itself, nothing changed.
    let output = jadpo(&root, &["approval", "--before", "HEAD"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert!(
        output.stdout.starts_with("No decision needs a person\n"),
        "{}",
        output.stdout
    );
    assert!(
        output.stdout.contains("  source: no change\n"),
        "{}",
        output.stdout
    );
    let unknown = jadpo(&root, &["approval", "--before", "no-such-branch"]);
    assert_eq!(unknown.code, 2, "{}", unknown.stdout);
    let said = format!("{}{}", unknown.stdout, unknown.stderr);
    assert!(said.contains("cli.approval_arguments"), "{said}");
    assert!(
        said.contains("`no-such-branch` isn't a branch or commit"),
        "{said}"
    );
}

#[test]
fn with_nothing_to_compare_with_no_report_is_written() {
    let root = library("approval-nothing");
    // A repository with no commit yet.
    fs::remove_dir_all(root.join(".git")).expect("removed");
    git(&root, &["init", "-q"]);
    let output = jadpo(&root, &["approval"]);
    assert_eq!(output.code, 2, "{}", output.stdout);
    let said = format!("{}{}", output.stdout, output.stderr);
    assert!(
        said.contains("Nothing to compare with: no commit yet"),
        "{said}"
    );
    assert!(said.contains("cli.approval_arguments"), "{said}");
    assert!(!root.join("build/review").exists());
}

#[test]
fn a_source_with_errors_writes_no_report() {
    let root = changed("approval-errors");
    edit(
        &root,
        "domains/lending/actions/lend.jadpo",
        "assert lent.borrower_id == member_id",
        "assert lent.borower_id == member_id",
    );
    let output = jadpo(&root, &["approval"]);
    assert_eq!(output.code, 1, "{}", output.stdout);
    assert!(
        output.stdout.contains("approval not written"),
        "{}",
        output.stdout
    );
    assert!(!root.join("build/review/approval.md").exists());
}
