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
        "/// Someone who writes notes.\nentity Author {\n    id: Uuid\n    name: AuthorName\n}\n",
    );
    write(
        &root,
        "domains/notes/entities/note.jadpo",
        "/// A note.\nentity Note {\n    id: Uuid\n    author_id: Author.id {\n        on_delete: OnDelete.cascade\n    }\n    body: NoteBody\n}\n",
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
        "/// An author as a note shows them.\ntype AuthorView = Output {\n    pick: [Author.name]\n}\n",
    );
    write(
        &root,
        "domains/notes/types/note_view.jadpo",
        "/// A note with its author.\ntype NoteView = Output {\n    pick: [Note.id, Note.body]\n    author: AuthorView\n}\n",
    );
    write(
        &root,
        "domains/notes/queries/notes_by_author.jadpo",
        "/// An author's notes.\nquery notes_by_author(author_id: Author.id) -> List<NoteView> {\n    from: Note\n    where: Note.author_id == author_id\n    order: [Note.body asc]\n    limit: 20\n    include: {\n        author: Note.author_id\n    }\n}\n",
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
        "query notes_by_author\n  SELECT \"id\", \"body\", \"author_id\"\n  FROM \"note\"\n  WHERE \"author_id\" = $1\n  ORDER BY \"body\" COLLATE \"C\" ASC, \"id\" ASC\n  LIMIT 20\n\n  reads: Note, a list of NoteView\n  file: domains/notes/queries/notes_by_author.jadpo:2\n  sends: 2 statements: this one, and one for each include\n  $1: author_id · uuid\n\ninclude author\n  SELECT \"id\", \"name\"\n  FROM \"author\"\n  WHERE \"id\" = ANY($1)\n\n  follows: Note.author_id, the Author it points at\n  $1: each Note's author_id · uuid[]\n"
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
        missing.stderr.contains("`jadpo sql` needs a query"),
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
        "/// Broken.\nquery broken(body: Note.body) -> List<Note> {\n    from: Note\n    where: Note.title == body\n    order: [Note.body asc]\n    limit: 5\n}\n",
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
