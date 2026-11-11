//! What `jadpo check` writes to `build/` for review and for tools (area
//! 14, "The `build/` folder"; chunk T4): the schema report, every table,
//! column and index with the queries that read it; the query report, each
//! statement's rows and index, each query call and the most queries each
//! route, action, schedule and subscriber sends, where a threshold's
//! `queries:` starts (area 16, "Setting and changing them"); the error
//! catalogue in `diagnostics/`; and `inventory/`, `audit/` and
//! `compatibility/`, made of the manifest's own entries.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("build-reports-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap_or_else(|_| panic!("`{path}` was written"))
}

fn json(root: &Path, path: &str) -> serde_json::Value {
    serde_json::from_str(&read(root, path)).expect("json")
}

/// Authors and their notes: a route reading an author's notes through a
/// query with an include, an action reading an author and their notes, a
/// count of every note no index serves, an archive that can refuse, and a
/// threshold holding the reading action to fewer queries than it sends.
fn project(name: &str) -> PathBuf {
    let root = folder(name);
    let files: [(&str, &str); 17] = [
        ("app.jadpo", "application {}\n"),
        (
            "domains/notes/entities/author.jadpo",
            "use notes/types { AuthorName }\n\n/// Someone who writes notes.\nentity Author {\n    id: Uuid\n    name: AuthorName\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/notes/entities/note.jadpo",
            "use notes/entities { Author }\nuse notes/types { NoteBody }\n\n/// A note.\nentity Note {\n    id: Uuid\n    author_id: Author.id {\n        on_delete: OnDelete.cascade\n    }\n    body: NoteBody\n    archived_at: Instant?\n\n    policy: {\n        read: [Access.public]\n        update: [Access.public]\n    }\n\n    lifecycle: {\n        transitions: {\n            archive: {\n                when: archived_at == none\n                set: {\n                    archived_at: clock.now\n                }\n            }\n        }\n    }\n}\n",
        ),
        (
            "domains/notes/types/author_name.jadpo",
            "/// An author's name.\ntype AuthorName = Text {\n    max_length: 80\n}\n",
        ),
        (
            "domains/notes/types/note_body.jadpo",
            "/// What a note says.\ntype NoteBody = Text {\n    max_length: 500\n}\n",
        ),
        (
            "domains/notes/types/author_view.jadpo",
            "use notes/entities { Author }\n\n/// An author as a note shows them.\ntype AuthorView = Output {\n    pick: [Author.name]\n}\n",
        ),
        (
            "domains/notes/types/note_view.jadpo",
            "use notes/entities { Note }\nuse notes/types { AuthorView }\n\n/// A note with its author.\ntype NoteView = Output {\n    pick: [Note.id, Note.body]\n    author: AuthorView\n}\n",
        ),
        (
            "domains/notes/types/note_count.jadpo",
            "/// How many notes there are.\ntype NoteCount = Output {\n    notes: Int\n}\n",
        ),
        (
            "domains/notes/types/archived_note.jadpo",
            "use notes/entities { Note }\n\n/// A note just archived.\ntype ArchivedNote = Output {\n    pick: [Note.id]\n}\n",
        ),
        (
            "domains/notes/queries/notes_by_author.jadpo",
            "use notes/entities { Author, Note }\nuse notes/types { NoteView }\n\n/// An author's notes.\nquery notes_by_author(author_id: Author.id) -> List<NoteView> {\n    from: Note\n    where: Note.author_id == author_id\n    order: [Note.body asc]\n    limit: 20\n    include: {\n        author: Note.author_id\n    }\n}\n",
        ),
        (
            "domains/notes/queries/note_tally.jadpo",
            "use notes/entities { Note }\nuse notes/types { NoteCount }\n\n/// Every note, counted.\nquery note_tally() -> NoteCount {\n    from: Note\n    aggregate: {\n        notes: count()\n    }\n}\n",
        ),
        (
            "domains/notes/failures/note_archived.jadpo",
            "/// The note is archived already.\ntype NoteArchived = Failure {\n    kind: FailureKind.conflict\n}\n",
        ),
        (
            "domains/notes/actions/archive_note.jadpo",
            "use notes/entities { Note, NoteNotFound }\nuse notes/failures { NoteArchived }\nuse notes/types { ArchivedNote }\n\n/// Archives a note, by id.\naction archive_note(note_id: Note.id) fails NoteNotFound, NoteArchived -> ArchivedNote {\n    var archived: Note = attempt Note.archive(note_id) {\n        conflict: {\n            Note.archive: NoteArchived\n        }\n    }\n    return ArchivedNote {\n        id: archived.id\n    }\n}\n",
        ),
        (
            "domains/notes/actions/author_notes.jadpo",
            "use notes/entities { Author, AuthorNotFound }\nuse notes/types { NoteView }\nuse notes/queries { notes_by_author }\n\n/// Reads an author, then their notes.\naction author_notes(author_id: Author.id) fails AuthorNotFound -> List<NoteView> {\n    var author: Author = attempt Author.get(author_id)\n    return notes_by_author(author.id)\n}\n",
        ),
        (
            "domains/notes/routes/notes.jadpo",
            "use notes/entities { Author, Note }\nuse notes/types { ArchivedNote, NoteView }\nuse notes/actions { archive_note }\nuse notes/queries { notes_by_author }\n\n/// Notes.\nroutes Notes {\n    prefix: /notes\n\n    /// An author's notes.\n    GET /by/{author_id: Author.id} {\n        authentication: RouteAuthentication.none\n        output: List<NoteView>\n        run: notes_by_author(author_id)\n    }\n\n    /// Archives a note.\n    POST /{note_id: Note.id}/archive {\n        authentication: RouteAuthentication.none\n        output: ArchivedNote\n        run: archive_note(note_id)\n    }\n}\n",
        ),
        (
            "domains/notes/monitoring.jadpo",
            "use notes/actions { author_notes }\n\nthresholds {\n    /// Reading an author's notes stays cheap.\n    reading: {\n        watches: author_notes\n        queries: 2\n    }\n}\n",
        ),
        (
            "policy.jadpo",
            "use notes\n\npolicy {\n    actions: {\n        notes.author_notes: [Access.public]\n    }\n}\n",
        ),
    ];
    for (path, text) in files {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}", migrated.stdout);
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    root
}

#[test]
fn the_schema_report_lists_every_table_column_and_index_with_its_readers() {
    let root = project("schema");
    let report = read(&root, "build/report/database.md");
    assert!(report.starts_with("# Database\n\n"), "{report}");
    assert!(report.contains("## `main`\n\n"), "{report}");
    assert!(
        report.contains("### `note` · entity `Note`\n\n"),
        "{report}"
    );
    assert!(
        report.contains("| `author_id` | `uuid` | no | `author_id` |\n"),
        "{report}"
    );
    assert!(
        report.contains("| `archived_at` | `timestamptz(3)` | yes | `archived_at` |\n"),
        "{report}"
    );
    // Jadpo's own tables are named, not spelled out.
    assert!(
        report.contains("Jadpo's own tables, whose columns"),
        "{report}"
    );
    assert!(!report.contains("### `jadpo_outbox`"), "{report}");
    // The index the query needed, in the order it sorts text, read by it.
    assert!(
        report.contains(
            "| `note_author_id_body_id_sorted_idx` | `author_id`, `body` in code-point order, `id` | needed by query `notes_by_author` | `notes_by_author` |\n"
        ),
        "{report}"
    );
    // The author's key serves the include.
    assert!(
        report.contains(
            "| `author_pkey` | `id` | primary key | `notes_by_author` include `author` |\n"
        ),
        "{report}"
    );
    assert!(
        report.contains(
            "| `note_author_id_fkey` | `author_id` | refers to `author`, on delete cascade | – |\n"
        ),
        "{report}"
    );
    let value = json(&root, "build/report/database.json");
    let tables = value["stores"][0]["tables"].as_array().expect("tables");
    let note = tables
        .iter()
        .find(|table| table["name"] == "note")
        .expect("the note table");
    assert_eq!(note["entity"], "Note");
    let index = note["indexes"]
        .as_array()
        .expect("indexes")
        .iter()
        .find(|index| index["name"] == "note_author_id_body_id_sorted_idx")
        .expect("the query's index");
    assert_eq!(index["readBy"][0], "notes_by_author");
}

#[test]
fn the_query_report_gives_each_reads_cost_and_where_a_threshold_starts() {
    let root = project("queries");
    let report = read(&root, "build/report/queries.md");
    assert!(report.starts_with("# Queries\n\n"), "{report}");
    // The count no index serves is the first thing to look at.
    assert!(report.contains("## Reads every record\n\n"), "{report}");
    assert!(
        report.contains("- `note_tally` reads every `Note` in `note`\n"),
        "{report}"
    );
    assert!(
        report.contains(
            "| `notes_by_author` | `Note` | `main` | up to 20 | `note_author_id_body_id_sorted_idx` |\n"
        ),
        "{report}"
    );
    assert!(
        report.contains(
            "| `notes_by_author` include `author` | `Author` | `main` | 1 per `Note` | `author_pkey` |\n"
        ),
        "{report}"
    );
    assert!(
        report.contains("| `note_tally` | `Note` | `main` | 1 | none: every record |\n"),
        "{report}"
    );
    // Each call, with where it reads from.
    assert!(
        report.contains(
            "| `notes_by_author` | `action author_notes` | `domains/notes/actions/author_notes.jadpo:8` | the store, in the action's transaction |\n"
        ),
        "{report}"
    );
    assert!(
        report.contains(
            "| `notes_by_author` | `route GET /notes/by/{author_id}` | `domains/notes/routes/notes.jadpo:11` | the store |\n"
        ),
        "{report}"
    );
    // The most each sends, and where a threshold starts or stands.
    assert!(
        report.contains("| `route GET /notes/by/{author_id}` | 2 | start at `queries: 2` |\n"),
        "{report}"
    );
    assert!(
        report.contains("| `route POST /notes/{note_id}/archive` | 0 | – |\n"),
        "{report}"
    );
    assert!(
        report.contains(
            "| `action author_notes` | 3 | `queries: 2` in `notes.reading`, below the 3 it can send |\n"
        ),
        "{report}"
    );
    let value = json(&root, "build/report/queries.json");
    let runs = value["runs"].as_array().expect("runs");
    let route = runs
        .iter()
        .find(|run| run["runs"] == "route GET /notes/by/{author_id}")
        .expect("the route");
    assert_eq!(route["queries"], 2);
    assert_eq!(route["startingQueries"], 2);
    let action = runs
        .iter()
        .find(|run| run["runs"] == "action author_notes")
        .expect("the action");
    assert_eq!(action["threshold"]["name"], "notes.reading");
    assert_eq!(action["threshold"]["queries"], 2);
    let statements = value["statements"].as_array().expect("statements");
    let tally = statements
        .iter()
        .find(|s| s["query"] == "note_tally")
        .expect("the count");
    assert_eq!(tally["index"], serde_json::Value::Null);
}

#[test]
fn inventory_audit_and_compatibility_are_views_of_the_manifest() {
    let root = project("views");
    let routes = json(&root, "build/inventory/routes.json");
    let ids: Vec<&str> = routes["routes"]
        .as_array()
        .expect("routes")
        .iter()
        .filter_map(|route| route["id"].as_str())
        .collect();
    assert!(ids.contains(&"route GET /notes/by/{author_id}"), "{ids:?}");
    let callables = json(&root, "build/inventory/callables.json");
    assert!(
        callables["callables"]
            .as_array()
            .expect("callables")
            .iter()
            .any(|c| c["id"] == "action archive_note" && c["kind"] == "action"),
        "{callables}"
    );
    let transactions = json(&root, "build/audit/transactions.json");
    let archive = transactions["actions"]
        .as_array()
        .expect("actions")
        .iter()
        .find(|a| a["id"] == "action archive_note")
        .expect("archive_note");
    assert_eq!(archive["writes"][0], "entity Note");
    for path in [
        "build/audit/failures.json",
        "build/audit/entities.json",
        "build/audit/configuration.json",
        "build/audit/policy.json",
    ] {
        assert!(json(&root, path)["format"].is_i64(), "{path}");
    }
    let codes = json(&root, "build/compatibility/public-failure-codes.json");
    let archived = codes["codes"]
        .as_array()
        .expect("codes")
        .iter()
        .find(|code| code["code"] == "note_archived")
        .expect("the archive's conflict");
    assert_eq!(archived["status"], 409);
    assert_eq!(archived["routes"][0], "route POST /notes/{note_id}/archive");
}

#[test]
fn the_error_catalogue_holds_the_last_checks_errors() {
    let root = project("errors");
    let clean = json(&root, "build/diagnostics/errors.json");
    assert_eq!(clean["errors"], 0);
    assert_eq!(clean["diagnostics"].as_array().map(Vec::len), Some(0));
    write(
        &root,
        "domains/notes/types/note_body.jadpo",
        "/// What a note says.\ntype NoteBody = Txt {\n    max_length: 500\n}\n",
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1);
    let errors = json(&root, "build/diagnostics/errors.json");
    assert!(
        errors["errors"].as_i64().unwrap_or_default() >= 1,
        "{errors}"
    );
    assert!(
        errors["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|d| d["rule"] == "semantic.unknown_name"
                && d["location"]["path"] == "domains/notes/types/note_body.jadpo"),
        "{errors}"
    );
    // A project with errors writes no reports it can't be sure of.
    assert!(!root.join("build/report/database.md").exists());
    assert!(!root.join("build/report/queries.md").exists());
}
