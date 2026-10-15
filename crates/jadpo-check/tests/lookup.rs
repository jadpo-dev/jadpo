//! What the name at a place refers to (for the language server's hover):
//! declarations where they are written and where they are used, fields and
//! variants, library functions, and parameters and variables.

use crate::support;

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
        "use notes/types { Body }\n\ntype Note = Object {\n    body: Body\n    pinned: Bool\n}\n",
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
        "use notes/types { Body }\n\nlibrary wording {\n    function keep(body: Body) -> Body {\n        return body\n    }\n}\n",
    ),
    (
        "domains/notes/actions/save_note.jadpo",
        "use notes/types { Body, Note }\nuse notes/failures { NoteRefused }\nuse notes/libraries { wording }\n\naction save_note(body: Body) fails NoteRefused -> Note {\n    if (body == Body(\"no\")) {\n        reject NoteRefused {\n            length: 2\n        }\n    }\n    var kept: Body = wording.keep(body)\n    return Note {\n        body: kept\n        pinned: false\n    }\n}\n",
    ),
    (
        "domains/notes/actions/shade_of.jadpo",
        "use notes/types { Note, Shade }\n\naction shade_of(note: Note) -> Shade {\n    if (note.pinned) {\n        return Shade.plain\n    }\n    return Shade.plain\n}\n",
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
fn names_on_use_lines() {
    assert_eq!(node(SAVE, "Body, Note }", 0), "type Body");
    assert_eq!(node(SAVE, "Note }", 0), "type Note");
    assert_eq!(node(SAVE, "NoteRefused }", 0), "type NoteRefused");
    assert_eq!(node(SAVE, "wording }", 0), "library wording");
    // The folder isn't a node.
    assert_eq!(at(SAVE, "failures {", 0), None);
}

/// Area 1: every name clicks through. Each name a file writes that the
/// project declares, wherever it stands (a `use` line, a signature, a
/// type's block, a body), leads to a node of the graph with a place in a
/// file.
#[test]
fn every_use_of_a_declared_name_leads_to_its_declaration() {
    use jadpo_check::graph::graph;
    use jadpo_syntax::SyntaxKind;

    let db = Database::default();
    let project = support::project(&db, Layout::Project, FILES);
    assert!(jadpo_check::check(&db, project).is_empty(), "checks clean");
    let built = graph(&db, project);
    let declared: Vec<&str> = [
        "Body",
        "Note",
        "Shade",
        "NoteRefused",
        "wording",
        "keep",
        "save_note",
        "shade_of",
    ]
    .to_vec();
    let mut seen = 0;
    for file in project.files(&db).iter().copied() {
        let tree = jadpo_db::parse(&db, file).syntax();
        for token in tree
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == SyntaxKind::IDENT)
            .filter(|token| declared.contains(&token.text()))
        {
            let offset = u32::from(token.text_range().start()) + 1;
            let found = lookup(&db, project, file, offset).map(|(found, _)| found);
            let Some(Found::Node(key)) = found else {
                panic!("`{}` in {}: {found:?}", token.text(), file.path(&db));
            };
            let place = built.get(&key).and_then(|node| node.place.clone());
            assert!(
                place.is_some(),
                "`{}` in {}: no place",
                token.text(),
                file.path(&db)
            );
            seen += 1;
        }
    }
    assert!(seen > 20, "{seen}");
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
    // A field given while building a value.
    assert_eq!(node(SAVE, "pinned: false", 0), "field Note.pinned");
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

/// The review's case: a variable in an inner block has the name of a
/// function the outer block then calls. The checker, the graph and the
/// language server read one resolution, the typed body's, so all three say
/// the call reaches the function and the variable is a variable.
#[test]
fn a_shadowing_variable_and_a_call_agree_everywhere() {
    use jadpo_check::graph::graph;
    use jadpo_graph::{EdgeKind, Key, Kind};

    let source = "type Num = Int {\n    min: 0\n}\n\ntype Flag = Bool {}\n\nfunction total(x: Num) -> Num {\n    return x\n}\n\nfunction pick(b: Flag, x: Num) -> Num {\n    if (b) {\n        var total: Num = x\n        return total\n    }\n    return total(x)\n}\n";
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", source)]);
    assert!(jadpo_check::check(&db, project).is_empty(), "checks clean");

    let built = graph(&db, project);
    let pick = built.find(&Key::new(Kind::Function, "pick")).expect("pick");
    let called: Vec<String> = built
        .outgoing(pick, EdgeKind::Calls)
        .map(|id| built.node(id).key.to_string())
        .collect();
    assert_eq!(called, ["function total"]);

    let file = project.files(&db)[0];
    let offset = |needle: &str, nth: usize| {
        source.match_indices(needle).nth(nth).expect("needle").0 as u32 + 1
    };
    let found = |offset| lookup(&db, project, file, offset).map(|(found, _)| found);
    assert_eq!(
        found(offset("total(x)", 0)),
        Some(Found::Node(Key::new(Kind::Function, "total")))
    );
    assert_eq!(
        found(offset("return total\n", 0) + 7),
        Some(Found::Local {
            name: "total".into(),
            ty: Some("Num".into()),
            what: "variable",
        })
    );
}
