//! What the name at a place refers to (for the language server's hover):
//! declarations where they are written and where they are used, fields and
//! variants, library functions, and parameters and variables.

mod support;

use jadpo_check::graph::{Found, lookup};
use jadpo_db::{Database, Layout};

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/notes/types/body.jadpo",
        "type Body = Text {\n    min_length: 1\n}\n",
    ),
    (
        "domains/notes/types/note.jadpo",
        "type Note = Object {\n    body: Body\n    pinned: Bool\n}\n",
    ),
    (
        "domains/notes/types/shade.jadpo",
        "type Shade = Enum {\n    plain\n    tinted: {\n        level: Int\n    }\n}\n",
    ),
    (
        "domains/notes/failures/note_refused.jadpo",
        "type NoteRefused = Failure {\n    kind: FailureKind.rejected\n    client: {\n        length: Int\n    }\n}\n",
    ),
    (
        "domains/notes/libraries/wording.jadpo",
        "library wording {\n    function keep(body: Body) -> Body {\n        return body\n    }\n}\n",
    ),
    (
        "domains/notes/actions/save_note.jadpo",
        "action save_note(body: Body) fails NoteRefused -> Note {\n    if (body == Body(\"no\")) {\n        reject NoteRefused {\n            length: 2\n        }\n    }\n    var kept: Body = wording.keep(body)\n    return Note {\n        body: kept\n        pinned: false\n    }\n}\n",
    ),
    (
        "domains/notes/actions/shade_of.jadpo",
        "action shade_of(note: Note) -> Shade {\n    if (note.pinned) {\n        return Shade.plain\n    }\n    return Shade.plain\n}\n",
    ),
];

/// What the name at the `nth` (from 0) `needle` in `path` refers to; the
/// offset is one character into the needle.
fn at(path: &str, needle: &str, nth: usize) -> Option<Found> {
    let db = Database::default();
    let project = support::project(&db, Layout::Project, FILES);
    let file = project
        .files(&db)
        .iter()
        .copied()
        .find(|file| file.path(&db) == path)
        .expect("file");
    let text = file.text(&db).clone();
    let offset = text
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("{needle} #{nth} in {path}"))
        .0 as u32
        + 1;
    lookup(&db, project, file, offset).map(|(found, _)| found)
}

fn node(path: &str, needle: &str, nth: usize) -> String {
    match at(path, needle, nth) {
        Some(Found::Node(key)) => key.to_string(),
        other => panic!("{needle} in {path}: {other:?}"),
    }
}

const SAVE: &str = "domains/notes/actions/save_note.jadpo";

#[test]
fn declarations_where_they_are_written_and_used() {
    assert_eq!(node(SAVE, "save_note", 0), "action save_note");
    assert_eq!(node(SAVE, "Body", 0), "type Body");
    assert_eq!(node(SAVE, "NoteRefused", 0), "type NoteRefused");
    assert_eq!(node(SAVE, "NoteRefused", 1), "type NoteRefused");
    assert_eq!(node(SAVE, "Note {", 0), "type Note");
    assert_eq!(
        node("domains/notes/types/body.jadpo", "Body", 0),
        "type Body"
    );
}

#[test]
fn libraries_and_their_functions() {
    assert_eq!(node(SAVE, "wording", 0), "library wording");
    assert_eq!(node(SAVE, "keep", 0), "function wording.keep");
    assert_eq!(
        node("domains/notes/libraries/wording.jadpo", "keep", 0),
        "function wording.keep"
    );
    assert_eq!(
        node("domains/notes/libraries/wording.jadpo", "wording", 0),
        "library wording"
    );
}

#[test]
fn fields_and_variants() {
    assert_eq!(
        node("domains/notes/types/note.jadpo", "body", 0),
        "field Note.body"
    );
    assert_eq!(
        node("domains/notes/failures/note_refused.jadpo", "length", 0),
        "field NoteRefused.length"
    );
    assert_eq!(
        node("domains/notes/types/shade.jadpo", "tinted", 0),
        "variant Shade.tinted"
    );
    assert_eq!(
        node("domains/notes/types/shade.jadpo", "level", 0),
        "field Shade.tinted.level"
    );
    let shade_of = "domains/notes/actions/shade_of.jadpo";
    assert_eq!(node(shade_of, "plain", 0), "variant Shade.plain");
    assert_eq!(node(shade_of, "pinned", 0), "field Note.pinned");
}

#[test]
fn parameters_and_variables() {
    assert_eq!(
        at(SAVE, "body ==", 0),
        Some(Found::Local {
            name: "body".into(),
            ty: Some("Body".into()),
            what: "parameter",
        })
    );
    assert_eq!(
        at(SAVE, "kept\n", 0),
        Some(Found::Local {
            name: "kept".into(),
            ty: Some("Body".into()),
            what: "variable",
        })
    );
    // Nothing to say about keywords, literals or the standard library.
    assert_eq!(at(SAVE, "return", 0), None);
    assert_eq!(
        at(
            "domains/notes/failures/note_refused.jadpo",
            "FailureKind",
            0
        ),
        None
    );
}
