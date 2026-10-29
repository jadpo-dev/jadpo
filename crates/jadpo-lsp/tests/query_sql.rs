//! Hovering over a query shows its generated SQL (area 6; area 14,
//! "Editor support"), the statements `jadpo sql` prints, read from the
//! same plan; never written into the source. As `jadpo sql` does, it
//! waits for a project without errors.

use serde_json::json;

use crate::support::{ask, at, project, start, uri};

const NOTES: &str = "notes.jadpo";
const NOTES_TEXT: &str = "type NoteBody = Text {\n    max_length: 500\n}\n\ntype AuthorName = Text {\n    max_length: 80\n}\n\nentity Author {\n    id: Uuid\n    name: AuthorName\n\n    policy: {\n        read: [Access.public]\n    }\n}\n\nentity Note {\n    id: Uuid\n    author_id: Author.id {\n        on_delete: OnDelete.cascade\n    }\n    body: NoteBody\n\n    policy: {\n        read: [Access.public]\n    }\n}\n\ntype AuthorView = Output {\n    pick: [Author.name]\n}\n\ntype NoteView = Output {\n    pick: [Note.id, Note.body]\n    author: AuthorView\n}\n\n/// Every note, by body.\nquery recent_notes() -> List<NoteView> {\n    from: Note\n    order: [Note.body asc]\n    limit: 20\n    include: {\n        author: Note.author_id\n    }\n}\n\naction latest() -> List<NoteView> {\n    return recent_notes()\n}\n";

fn hover(text: &str, needle: &str, nth: usize) -> String {
    let root = project(&format!("query-sql-{}", text.len()), &[(NOTES, text)]);
    let mut server = start(&root, json!({}));
    let answer = ask(
        &mut server,
        "textDocument/hover",
        json!({
            "textDocument": {"uri": uri(&root, NOTES)},
            "position": at(text, needle, nth, 2),
        }),
    );
    answer["contents"]["value"]
        .as_str()
        .expect("markdown")
        .to_string()
}

#[test]
fn a_query_shows_each_statement_it_sends() {
    let shown = hover(NOTES_TEXT, "recent_notes", 0);
    assert!(
        shown.contains("query recent_notes() -> List<NoteView>"),
        "{shown}"
    );
    assert!(shown.contains("sends 2 statements"), "{shown}");
    let query = shown
        .find("```sql\n-- query recent_notes\nSELECT")
        .expect(&shown);
    let include = shown
        .find("```sql\n-- include author\nSELECT")
        .expect(&shown);
    assert!(query < include, "{shown}");
    assert!(shown.contains("FROM \"note\""), "{shown}");
    assert!(shown.contains("LIMIT 20"), "{shown}");
}

#[test]
fn a_call_of_a_query_shows_it_too() {
    let shown = hover(NOTES_TEXT, "recent_notes", 1);
    assert!(shown.contains("-- query recent_notes"), "{shown}");
}

#[test]
fn the_sql_waits_for_a_project_without_errors() {
    let broken = NOTES_TEXT.replace("return recent_notes()", "return recnt_notes()");
    let shown = hover(&broken, "recent_notes", 0);
    assert!(!shown.contains("```sql"), "{shown}");
    assert!(
        shown.contains("SQL shown once the project has no errors"),
        "{shown}"
    );
}

#[test]
fn other_declarations_show_no_sql() {
    let shown = hover(NOTES_TEXT, "Note {", 0);
    assert!(!shown.contains("```sql"), "{shown}");
}
