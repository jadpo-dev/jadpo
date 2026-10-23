//! `jadpo sql <query>` (area 6; area 14's command table): the statements a
//! query sends, one for the query and one for each include, each with its
//! parameters, in plain output and as `--json`; and its argument errors.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("sql-{name}"));
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
    run(
        &args,
        &Environment::empty()
            .with_var("SOURCE_DATE_EPOCH", "1792504800")
            .with_cwd(cwd),
    )
}

/// A project with a user, their notes and a query including the user.
fn project(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/notes/entities/author.jadpo",
        "use notes/types { AuthorName }\n\n/// Someone who writes notes.\nentity Author {\n    id: Uuid\n    name: AuthorName\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
    );
    write(
        &root,
        "domains/notes/entities/note.jadpo",
        "use notes/entities { Author }\nuse notes/types { NoteBody }\n\n/// A note.\nentity Note {\n    id: Uuid\n    author_id: Author.id {\n        on_delete: OnDelete.cascade\n    }\n    body: NoteBody\n    archived_at: Instant?\n\n    policy: {\n        read: [Access.public]\n        update: [Access.public]\n    }\n\n    lifecycle: {\n        transitions: {\n            archive: {\n                when: archived_at == none\n                set: {\n                    archived_at: clock.now\n                }\n            }\n        }\n    }\n}\n",
    );
    write(
        &root,
        "domains/notes/types/author_name.jadpo",
        "/// An author's name.\ntype AuthorName = Text {\n    max_length: 80\n}\n",
    );
    write(
        &root,
        "domains/notes/types/note_body.jadpo",
        "/// What a note says.\ntype NoteBody = Text {\n    max_length: 500\n}\n",
    );
    write(
        &root,
        "domains/notes/types/author_view.jadpo",
        "use notes/entities { Author }\n\n/// An author as a note shows them.\ntype AuthorView = Output {\n    pick: [Author.name]\n}\n",
    );
    write(
        &root,
        "domains/notes/types/note_view.jadpo",
        "use notes/entities { Note }\nuse notes/types { AuthorView }\n\n/// A note with its author.\ntype NoteView = Output {\n    pick: [Note.id, Note.body]\n    author: AuthorView\n}\n",
    );
    write(
        &root,
        "domains/notes/queries/notes_by_author.jadpo",
        "use notes/entities { Author, Note }\nuse notes/types { NoteView }\n\n/// An author's notes.\nquery notes_by_author(author_id: Author.id) -> List<NoteView> {\n    from: Note\n    where: Note.author_id == author_id\n    order: [Note.body asc]\n    limit: 20\n    include: {\n        author: Note.author_id\n    }\n}\n",
    );
    write(
        &root,
        "domains/notes/failures/note_archived.jadpo",
        "/// The note is archived already.\ntype NoteArchived = Failure {\n    kind: FailureKind.conflict\n}\n",
    );
    write(
        &root,
        "domains/notes/actions/add_note.jadpo",
        "use notes/entities { Author, Note }\n\n/// Adds a note.\naction add_note(author: Author, body: Note.body) -> Note {\n    return Note.create {\n        set: {\n            author_id: author.id\n            body: body\n        }\n    }\n}\n",
    );
    write(
        &root,
        "domains/notes/actions/archive_note.jadpo",
        "use notes/entities { Note, NoteNotFound }\nuse notes/failures { NoteArchived }\n\n/// Archives a note, by id.\naction archive_note(note_id: Note.id) fails NoteNotFound, NoteArchived -> Note {\n    return attempt Note.archive(note_id) {\n        conflict: {\n            Note.archive: NoteArchived\n        }\n    }\n}\n",
    );
    write(
        &root,
        "domains/notes/actions/count_author.jadpo",
        "use notes/entities { Author, AuthorNotFound }\n\n/// Reads an author and writes nothing.\naction count_author(author_id: Author.id) fails AuthorNotFound -> Author {\n    return attempt Author.get(author_id)\n}\n",
    );
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}", migrated.stdout);
    root
}

#[test]
fn a_query_prints_each_statement_with_its_parameters() {
    let root = project("plain");
    let out = jadpo(&root, &["sql", "notes_by_author"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "query notes_by_author\n  SELECT \"id\", \"body\", \"author_id\"\n  FROM \"note\"\n  WHERE \"author_id\" = $1\n  ORDER BY \"body\" COLLATE \"C\" ASC, \"id\" ASC\n  LIMIT 20\n\n  reads: Note, a list of NoteView\n  file: domains/notes/queries/notes_by_author.jadpo:5\n  sends: 2 statements: this one, and one for each include\n  $1: author_id · uuid\n\ninclude author\n  SELECT \"id\", \"name\"\n  FROM \"author\"\n  WHERE \"id\" = ANY($1)\n\n  follows: Note.author_id, the Author it points at\n  $1: each Note's author_id · uuid[]\n"
    );
    let json = jadpo(&root, &["sql", "notes_by_author", "--json"]);
    let value: serde_json::Value = serde_json::from_str(&json.stdout).expect("json");
    let statements = &value["data"]["statements"];
    assert_eq!(statements.as_array().map(Vec::len), Some(2));
    assert_eq!(statements[1]["for"], "include author");
    assert_eq!(statements[0]["params"][0]["from"], "author_id");
    assert_eq!(statements[0]["params"][0]["type"], "uuid");
}

#[test]
fn a_missing_or_unknown_query_lists_the_ones_there_are() {
    let root = project("arguments");
    let missing = jadpo(&root, &["sql"]);
    assert_eq!(missing.code, 2);
    assert!(
        missing
            .stderr
            .contains("`jadpo sql` needs a query or an action"),
        "{}",
        missing.stderr
    );
    assert!(
        missing.stderr.contains("`notes_by_author`"),
        "{}",
        missing.stderr
    );
    let unknown = jadpo(&root, &["sql", "notes_by_autor"]);
    assert_eq!(unknown.code, 2);
    assert!(
        unknown.stderr.contains("Did you mean `notes_by_author`?"),
        "{}",
        unknown.stderr
    );
    assert!(unknown.stderr.contains("rule: cli.sql_arguments"));
    let explain = jadpo(&root, &["sql", "notes_by_author", "--explain"]);
    assert_eq!(explain.code, 1);
    assert!(
        explain.stderr.contains("rule: toolchain.not_built_yet"),
        "{}",
        explain.stderr
    );
}

#[test]
fn a_project_with_errors_shows_them_and_no_sql() {
    let root = project("errors");
    write(
        &root,
        "domains/notes/queries/broken.jadpo",
        "use notes/entities { Note }\n\n/// Broken.\nquery broken(body: Note.body) -> List<Note> {\n    from: Note\n    where: Note.title == body\n    order: [Note.body asc]\n    limit: 5\n}\n",
    );
    let out = jadpo(&root, &["sql", "notes_by_author"]);
    assert_eq!(out.code, 1);
    assert!(
        out.stdout.contains("rule: query.unknown_field"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("sql not shown"), "{}", out.stdout);
    assert!(!out.stdout.contains("SELECT"));
}

#[test]
fn an_action_prints_the_statement_each_write_sends() {
    let root = project("action");
    let out = jadpo(&root, &["sql", "archive_note"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "write Note.archive\n  UPDATE \"note\"\n  SET \"archived_at\" = $1\n  WHERE \"id\" = $2\n    AND \"archived_at\" IS NULL\n    AND ($3::text IS NULL OR \"xmin\"::text = $3)\n  RETURNING \"id\", \"author_id\", \"body\", \"archived_at\"\n\n  writes: runs Note.archive on the Note with that id\n  file: domains/notes/actions/archive_note.jadpo:6\n  sends: 1 statement\n  $1: clock.now · timestamptz\n  $2: note_id · uuid\n  $3: NULL::text · text\n  refused: when: of Note.archive → NoteArchived, mapped in conflict:\n  no row: NoteNotFound\n\nwhen no row changed\n  SELECT 1\n  FROM \"note\"\n  WHERE \"id\" = $1\n\n  then: a row means the record is there, so when: refused it → NoteArchived\n  $1: note_id · uuid\n"
    );
    let created = jadpo(&root, &["sql", "add_note", "--json"]);
    assert_eq!(created.code, 0, "{}", created.stderr);
    let value: serde_json::Value = serde_json::from_str(&created.stdout).expect("json");
    let statement = &value["data"]["statements"][0];
    assert_eq!(value["data"]["action"], "add_note");
    assert_eq!(statement["for"], "write Note.create");
    assert_eq!(statement["params"][1]["from"], "author.id");
    assert_eq!(
        statement["refusals"][0]["constraint"],
        "note_author_id_fkey"
    );
    let reads = jadpo(&root, &["sql", "count_author"]);
    assert_eq!(reads.code, 2);
    assert!(
        reads.stderr.contains("`count_author` writes nothing"),
        "{}",
        reads.stderr
    );
}

#[test]
fn tab_completes_queries_and_actions() {
    let root = project("complete");
    let out = jadpo(&root, &["__complete", "--", "sql", ""]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "add_note\tAction\narchive_note\tAction\ncount_author\tAction\nnotes_by_author\tQuery\n"
    );
}
