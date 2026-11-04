//! What a first record holds (area 18, "What a first record holds"; chunk
//! A15), end to end through git. A first commit has nothing to compare with,
//! so what would need a person later passes `jadpo check` unseen. Each such
//! thing is a notice marked `(human)`, once, and the approval report leads
//! with it: a decision, not a fault, so nothing fails.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::acceptances::{edit, example, git, jadpo, library, write};
use crate::http::tempdir;
use crate::installing::{release, workspace};

const BOOK: &str = "domains/lending/entities/book.jadpo";
const POLICY: &str = "policy.jadpo";

/// The library, its records written and in a repository with no commit yet.
fn fresh(name: &str) -> PathBuf {
    let root = library(&format!("first-{name}"));
    fs::remove_dir_all(root.join(".git")).expect("removed");
    git(&root, &["init", "-q", "-b", "main"]);
    root
}

fn update(root: &Path) {
    assert_eq!(jadpo(root, &["fmt"]).code, 0);
    let recorded = jadpo(root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}", recorded.stdout);
}

fn commit(root: &Path, message: &str) {
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", message]);
}

fn data(root: &Path) -> Value {
    let output = jadpo(root, &["check", "--json"]);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    assert_eq!(document["ok"], true, "{}", output.stdout);
    document["data"].clone()
}

/// What `jadpo check --json` says of each notice, `subject: said`.
fn notices(root: &Path) -> Vec<String> {
    data(root)["notices"]
        .as_array()
        .expect("notices")
        .iter()
        .map(|notice| {
            format!(
                "{}: {}",
                notice["subject"].as_str().unwrap_or(""),
                notice["said"].as_str().unwrap_or("")
            )
        })
        .collect()
}

fn public_create_and_update(root: &Path) {
    edit(
        root,
        BOOK,
        "create: [StaffRole.admin]",
        "create: [Access.public]",
    );
    edit(
        root,
        BOOK,
        "update: [StaffRole.admin]",
        "update: [StaffRole.admin, Access.public]",
    );
    update(root);
}

const PUBLIC: &str = "anyone can create and change `lending.Book` records, with no sign-in";

/// The first record with a public write says so until a commit holds it,
/// and passes: an app may mean it.
#[test]
fn a_new_projects_public_write_is_a_notice_for_a_person() {
    let root = fresh("public");
    public_create_and_update(&root);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    for words in [
        "1 decision compared with nothing",
        PUBLIC,
        "decision: nothing compares this, so a person reads it before it goes in",
        "(human)",
        "before: none, so acceptances aren't judged: no commit yet",
    ] {
        assert!(
            checked.stdout.contains(words),
            "{words}\n{}",
            checked.stdout
        );
    }
    // Before the summary, so it is read before anything says all is well.
    let said = checked.stdout.find(PUBLIC).expect("said");
    assert!(said < checked.stdout.find("checked").expect("the summary"));

    let found = data(&root);
    let notice = &found["notices"][0];
    assert_eq!(notice["subject"], "lending.Book");
    assert_eq!(notice["kind"], "public_write");
    assert_eq!(notice["said"], PUBLIC);
    assert_eq!(notice["decides"], "human");
    assert_eq!(notice["file"], "access.jadpo_record");
    let record = fs::read_to_string(root.join("access.jadpo_record")).expect("the record");
    let line = record
        .lines()
        .position(|line| line == "entity lending.Book")
        .expect("its entry")
        + 1;
    assert_eq!(notice["line"], line);

    // The first commit has nothing before it either, as CI sees it.
    commit(&root, "The library");
    assert_eq!(notices(&root), [format!("lending.Book: {PUBLIC}")]);
    // Once a commit holds it, it was seen, and is compared from then on.
    write(&root, "README.md", "The library.\n");
    commit(&root, "Say what it is");
    assert_eq!(notices(&root), Vec::<String>::new());
    git(&root, &["checkout", "-q", "-b", "later"]);
    assert_eq!(notices(&root), Vec::<String>::new());
    assert!(!jadpo(&root, &["check"]).stdout.contains("(human)"));
}

#[test]
fn reads_and_signed_in_writes_are_no_notice() {
    let root = fresh("reads");
    edit(
        &root,
        BOOK,
        "read: [Access.authenticated]",
        "read: [Access.public]",
    );
    edit(
        &root,
        BOOK,
        "create: [StaffRole.admin]",
        "create: [Access.authenticated]",
    );
    update(&root);
    assert_eq!(notices(&root), Vec::<String>::new());
    assert!(!jadpo(&root, &["check"]).stdout.contains("(human)"));
}

/// What `jadpo new` writes lists an action anyone may run, which writes
/// nothing: no one is asked about the scaffold.
#[test]
fn a_new_project_as_written_is_no_notice() {
    let parent = tempdir::Dir::new("first-new");
    assert_eq!(jadpo(parent.path(), &["new", "desk"]).code, 0);
    let root = parent.path().join("desk");
    git(&root, &["init", "-q", "-b", "main"]);
    assert_eq!(notices(&root), Vec::<String>::new());
}

/// A listed action's line replaces its entities' grants, so one anyone may
/// run that writes is a public write, through what it calls too.
#[test]
fn a_listed_action_anyone_can_run_that_writes_is_a_notice() {
    let root = fresh("listed");
    edit(
        &root,
        BOOK,
        "read: [Access.authenticated]",
        "read: [Access.public]",
    );
    edit(
        &root,
        POLICY,
        "lending.borrow: [Access.authenticated]",
        "lending.borrow: [Access.authenticated]\n        lending.shelve: [Access.public]\n        lending.put_back: [Access.public]\n        lending.look: [Access.public]",
    );
    write(
        &root,
        "domains/lending/actions/shelve.jadpo",
        "use lending/actions { put_back }\nuse lending/entities { Book, BookNotFound }\n\n/// Puts a book back, for anyone at the desk.\naction shelve(book_id: Book.id) fails BookNotFound -> Unit {\n    attempt put_back(book_id)\n}\n",
    );
    write(
        &root,
        "domains/lending/actions/put_back.jadpo",
        "use lending/entities { Book, BookNotFound }\n\n/// Clears who has a book.\naction put_back(book_id: Book.id) fails BookNotFound -> Unit {\n    var book: Book = attempt Book.get(book_id)\n    var back: Book = Book.update(book) {\n        set: {\n            borrower_id: none\n        }\n    }\n    assert back.id == book_id\n}\n",
    );
    write(
        &root,
        "domains/lending/actions/look.jadpo",
        "use lending/entities { Book, BookNotFound }\n\n/// Says whether a book is there.\naction look(book_id: Book.id) fails BookNotFound -> Unit {\n    var book: Book = attempt Book.get(book_id)\n    assert book.id == book_id\n}\n",
    );
    edit(
        &root,
        "domains/lending/routes/books.jadpo",
        "    POST /{book_id: Book.id}/borrow {\n",
        "    POST /{book_id: Book.id}/shelve {\n        authentication: RouteAuthentication.none\n        status: SuccessStatus.no_content\n        run: shelve(book_id)\n    }\n\n    POST /{book_id: Book.id}/look {\n        authentication: RouteAuthentication.none\n        status: SuccessStatus.no_content\n        run: look(book_id)\n    }\n\n    POST /{book_id: Book.id}/borrow {\n",
    );
    edit(
        &root,
        "domains/lending/routes/books.jadpo",
        "use lending/actions { borrow }\n",
        "use lending/actions { borrow, look, shelve }\n",
    );
    update(&root);
    let found = notices(&root);
    assert!(
        found.contains(
            &"lending.shelve: anyone can run `lending.shelve`, which creates, changes or deletes records, with no sign-in"
                .to_string()
        ),
        "{found:#?}"
    );
    assert!(
        !found.iter().any(|said| said.contains("lending.look")),
        "{found:#?}"
    );
}

/// A branch of a project whose base has no access record: compared with
/// nothing, so the same notice, beside what is judged.
#[test]
fn a_record_arriving_on_a_branch_says_so_too() {
    let parent = tempdir::Dir::new("first-branch");
    let root = parent.path().join("desk");
    write(&root, "app.jadpo", "application {}\n");
    git(&root, &["init", "-q", "-b", "main"]);
    commit(&root, "Nothing yet");
    git(&root, &["checkout", "-q", "-b", "change"]);
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n    }\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    /// The desk's database.\n    database_url: PostgresUrl\n}\n",
    );
    write(
        &root,
        "domains/desk/entities/note.jadpo",
        "/// A note anyone can leave.\nentity Note {\n    id: Uuid\n\n    policy: {\n        delete: [Access.public]\n    }\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    update(&root);
    assert_eq!(jadpo(&root, &["acceptance", "new", "the_desk"]).code, 0);
    let file = fs::read_dir(root.join("acceptances"))
        .expect("the folder")
        .flatten()
        .next()
        .expect("a draft")
        .path();
    let text = fs::read_to_string(&file).expect("read");
    fs::write(&file, format!("/// The first of it.\n{text}")).expect("written");
    let said = "anyone can delete `desk.Note` records, with no sign-in";
    assert_eq!(notices(&root), [format!("desk.Note: {said}")]);
    let checked = jadpo(&root, &["check"]);
    assert!(
        checked
            .stdout
            .contains("`access.jadpo_record` is new since"),
        "{}",
        checked.stdout
    );
}

/// The first record holding one, however old the record: a new entity's
/// grants need no acceptance, so the first public write there would pass
/// unseen too. A later one is the diff's, and a widening is a decision.
#[test]
fn the_first_public_write_in_a_record_already_there_says_so() {
    let root = library("first-in-record");
    let log = "/// What anyone left.\nentity Log {\n    id: Uuid\n\n    policy: {\n        create: [Access.public]\n    }\n}\n";
    write(&root, "domains/lending/entities/log.jadpo", log);
    assert_eq!(jadpo(&root, &["migrate", "new", "logs"]).code, 0);
    update(&root);
    let said = "anyone can create `lending.Log` records, with no sign-in";
    assert_eq!(notices(&root), [format!("lending.Log: {said}")]);

    commit(&root, "Logs");
    git(&root, &["checkout", "-q", "main"]);
    git(&root, &["merge", "-q", "--ff-only", "change"]);
    git(&root, &["checkout", "-q", "-b", "after"]);
    let next = "/// What anyone left next.\nentity Trace {\n    id: Uuid\n\n    policy: {\n        create: [Access.public]\n    }\n}\n";
    write(&root, "domains/lending/entities/trace.jadpo", next);
    assert_eq!(jadpo(&root, &["migrate", "new", "traces"]).code, 0);
    update(&root);
    assert_eq!(notices(&root), Vec::<String>::new());
}

/// A widening that lets anyone write is judged as ever, and isn't said twice.
#[test]
fn a_widening_is_a_decision_rather_than_a_notice() {
    let root = library("first-widened");
    edit(
        &root,
        BOOK,
        "create: [StaffRole.admin]",
        "create: [StaffRole.admin, Access.public]",
    );
    update(&root);
    let output = jadpo(&root, &["check", "--json"]);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    assert_eq!(document["ok"], false, "{}", output.stdout);
    assert_eq!(document["data"]["notices"], serde_json::json!([]));
    assert_eq!(
        document["diagnostics"][0]["rule"], "acceptance.missing",
        "{}",
        output.stdout
    );
}

/// With errors a person isn't asked yet: the agent's work comes first.
#[test]
fn a_project_with_errors_shows_no_notice() {
    let root = fresh("errors");
    public_create_and_update(&root);
    edit(
        &root,
        "domains/lending/types/handle.jadpo",
        "max_length: 40",
        "max_length: \"forty\"",
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(!checked.stdout.contains("(human)"), "{}", checked.stdout);
}

/// The rest that area 18 compares and a first commit doesn't, each in its
/// own words as the acceptance's message would say it.
#[test]
fn what_else_a_first_commit_passes_unseen_says_so() {
    let root = fresh("others");
    // A security rule declined.
    write(
        &root,
        "domains/lending/actions/lend.jadpo",
        "use lending/entities { Book, BookNotFound, Member, MemberNotFound }\n\n/// Lends a book to a member.\naction lend(book_id: Book.id, member_id: Member.id) fails BookNotFound, MemberNotFound -> Unit {\n    var book: Book = attempt Book.get(book_id)\n    var lent: Book = attempt Book.update(book) {\n        set: {\n            borrower_id: member_id\n        }\n    }\n    assert lent.borrower_id == member_id\n}\n",
    );
    write(
        &root,
        "domains/lending/actions/borrow.jadpo",
        "use lending/actions { lend }\nuse lending/entities { Book, BookNotFound, MemberNotFound }\n\n/// Borrows a book for the caller.\ndecline policy.call_ungranted: \"Only staff borrow through the desk\"\naction borrow(book_id: Book.id, caller: Principal.user) fails BookNotFound, MemberNotFound -> Unit {\n    attempt lend(book_id, caller.id)\n}\n",
    );
    // An own source.
    edit(
        &root,
        "app.jadpo",
        "application {\n",
        "application {\n    packages: {\n        own_sources: [\"github.com/acme/\"]\n    }\n",
    );
    update(&root);
    let found = notices(&root);
    let own = "\"github.com/acme/\": ";
    assert!(
        found.iter().any(|said| said.starts_with("lending.borrow: ")
            && said.contains("which declines `policy.call_ungranted`")),
        "{found:#?}"
    );
    assert!(found.iter().any(|said| said.starts_with(own)), "{found:#?}");
    // The app's own database is no decision: every app has one.
    assert!(
        !found.iter().any(|said| said.starts_with("main: ")),
        "{found:#?}"
    );
    let report = data(&root);
    let kinds: Vec<&str> = report["notices"]
        .as_array()
        .expect("notices")
        .iter()
        .filter_map(|notice| notice["kind"].as_str())
        .collect();
    assert!(
        kinds.contains(&"PolicyChange.call_ungranted_declined")
            && kinds.contains(&"PolicyChange.own_source_added"),
        "{kinds:?}"
    );
}

/// The teaching example `name` in a repository with no commit yet.
fn fresh_example(name: &str) -> PathBuf {
    let root = example(name);
    fs::remove_dir_all(root.join(".git")).expect("removed");
    git(&root, &["init", "-q", "-b", "main"]);
    root
}

#[test]
fn a_first_sign_in_setting_past_its_limit_says_so() {
    let root = fresh_example("27-guest-house");
    edit(
        &root,
        "authentication.jadpo",
        "magic_link: {\n",
        "magic_link: {\n            expires_after: 2h\n",
    );
    update(&root);
    let found = notices(&root);
    assert!(
        found
            .iter()
            .any(|said| said.starts_with("magic_link.expires_after: ")),
        "{found:#?}"
    );
}

/// An entity shared by every tenant, as the record says it.
#[test]
fn a_first_shared_entity_says_so() {
    let root = fresh_example("12-recipe-book");
    let found = notices(&root);
    assert!(
        found.iter().any(|said| said.starts_with("kitchen.Baker: ")),
        "{found:#?}"
    );
}

/// A package from a stranger's git passes a first commit unseen too.
#[test]
fn a_first_package_from_git_says_so() {
    let (_dir, package, app) = workspace("first-git");
    release(&package, "v2.0.0");
    let repository = package.to_string_lossy().into_owned();
    git(&app, &["init", "-q", "-b", "main"]);
    let added = jadpo(&app, &["package", "add", &format!("{repository}@v2.0.0")]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    edit(
        &app,
        "jadpo.lock",
        &format!(" git {repository} "),
        " git github.com/stranger/postcodes ",
    );
    let found = notices(&app);
    assert!(
        found.iter().any(|said| said.starts_with("Postcodes.v2: ")),
        "{found:#?}"
    );
}

/// The report leads with them, on a branch as on a project with no commit.
#[test]
fn the_approval_report_leads_with_a_notice() {
    let root = library("first-report");
    let log = "/// What anyone left.\nentity Log {\n    id: Uuid\n\n    policy: {\n        create: [Access.public]\n    }\n}\n";
    write(&root, "domains/lending/entities/log.jadpo", log);
    assert_eq!(jadpo(&root, &["migrate", "new", "logs"]).code, 0);
    update(&root);
    let said = "anyone can create `lending.Log` records, with no sign-in";
    let output = jadpo(&root, &["approval"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    let lead = output.stdout.find(said).expect("said on the terminal");
    assert!(lead < output.stdout.find("No decision").expect("what follows"));
    assert!(output.stdout.contains("(human)"), "{}", output.stdout);
    let markdown = fs::read_to_string(root.join("build/review/approval.md")).expect("the report");
    let lead = markdown
        .find("## Decisions compared with nothing")
        .expect("its section");
    assert!(
        lead < markdown
            .find("## Decisions that need a person")
            .expect("what follows")
    );
    assert!(markdown.contains("### Anyone can create `lending.Log` records, with no sign-in"));
    assert!(markdown.contains("`access.jadpo_record:"), "{markdown}");
    let json: Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/review/approval.json")).expect("the twin"),
    )
    .expect("JSON");
    assert_eq!(json["notices"][0]["subject"], "lending.Log");
    assert_eq!(json["notices"][0]["decides"], "human");
}

/// With no commit there is no before side to build, so the report holds what
/// nothing compared, and says that is all it holds.
#[test]
fn with_no_commit_the_report_is_what_nothing_compared() {
    let root = fresh("no-commit-report");
    public_create_and_update(&root);
    let output = jadpo(&root, &["approval"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    for words in [
        PUBLIC,
        "(human)",
        "no commit yet",
        "Wrote build/review/approval.md",
    ] {
        assert!(output.stdout.contains(words), "{words}\n{}", output.stdout);
    }
    let markdown = fs::read_to_string(root.join("build/review/approval.md")).expect("the report");
    assert!(
        markdown.contains("## Decisions compared with nothing"),
        "{markdown}"
    );
    assert!(markdown.contains("no commit yet"), "{markdown}");
    assert!(!markdown.contains("## Source by declaration"), "{markdown}");
    let json: Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/review/approval.json")).expect("the twin"),
    )
    .expect("JSON");
    assert_eq!(json["notices"][0]["subject"], "lending.Book");
    assert_eq!(json["before"]["not_judged_because"], "no commit yet");
}
