//! The graph (docs/compiler-runtime.md §2.3, area 18): every node's key and
//! digest survive formatting, moves within the layout rules and edits
//! elsewhere; the manifest is the same bytes whatever order the files come
//! in; a diff of two versions names what changed; and an edit rebuilds
//! only its own file's part.

mod support;

use std::collections::BTreeMap;

use jadpo_check::graph::graph;
use jadpo_db::{Database, Layout};
use jadpo_graph::diff::{What, diff};
use jadpo_graph::manifest::{Stamp, manifest};
use jadpo_graph::{EdgeKind, Graph, Key, Kind};
use salsa::Setter;

const APP: (&str, &str) = ("app.jadpo", "application {}\n");
const BODY_PATH: &str = "domains/notes/types/body.jadpo";
const BODY: &str =
    "/// The text of a note.\ntype Body = Text {\n    min_length: 1\n    max_length: 500\n}\n";
const NOTE_PATH: &str = "domains/notes/types/note.jadpo";
const NOTE: &str =
    "use notes/types { Body }\n\ntype Note = Object {\n    body: Body\n    pinned: Bool\n}\n";
const REFUSED_PATH: &str = "domains/notes/failures/note_refused.jadpo";
const REFUSED: &str = "/// The note was refused.\ntype NoteRefused = Failure {\n    kind: FailureKind.rejected\n    message: \"The note was refused.\"\n    client: {\n        length: Int\n    }\n}\n";
const WORDING_PATH: &str = "domains/notes/libraries/wording.jadpo";
const WORDING: &str = "use notes/types { Body }\n\nlibrary wording {\n    /// Keeps a body as it is.\n    function keep(body: Body) -> Body {\n        return body\n    }\n}\n";
const SAVE_PATH: &str = "domains/notes/actions/save_note.jadpo";
const SAVE: &str = "use notes/types { Body, Note }
use notes/failures { NoteRefused }
use notes/libraries { wording }

action save_note(body: Body) fails NoteRefused -> Note {
    if (body == Body(\"no\")) {
        reject NoteRefused {
            length: 2
        }
    }
    return Note {
        body: wording.keep(body)
        pinned: false
    }
}
";
const KIND_PATH: &str = "domains/notes/types/shade.jadpo";
const KIND: &str = "type Shade = Enum {\n    plain\n    tinted: {\n        level: Int\n    }\n}\n";
const SUMMARY_PATH: &str = "domains/notes/types/summary.jadpo";
const SUMMARY: &str = "use notes/types { Note, Shade }\n\ntype Summary = Object {\n    pick: [Note.body]\n    shade: Shade\n}\n";
const SUMMARISE_PATH: &str = "domains/notes/actions/summarise_note.jadpo";
const SUMMARISE: &str = "use notes/types { Note, Shade, Summary }\n\naction summarise_note(note: Note) -> Summary {\n    return Summary {\n        body: note.body\n        shade: Shade.plain\n    }\n}\n";

fn fixture() -> Vec<(&'static str, String)> {
    [
        APP,
        (BODY_PATH, BODY),
        (NOTE_PATH, NOTE),
        (REFUSED_PATH, REFUSED),
        (WORDING_PATH, WORDING),
        (SAVE_PATH, SAVE),
        (KIND_PATH, KIND),
        (SUMMARY_PATH, SUMMARY),
        (SUMMARISE_PATH, SUMMARISE),
    ]
    .into_iter()
    .map(|(path, text)| (path, text.to_string()))
    .collect()
}

fn with(
    files: &[(&'static str, String)],
    path: &'static str,
    text: &str,
) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> =
        files.iter().filter(|(p, _)| *p != path).cloned().collect();
    out.push((path, text.to_string()));
    out
}

fn without(files: &[(&'static str, String)], path: &str) -> Vec<(&'static str, String)> {
    files.iter().filter(|(p, _)| *p != path).cloned().collect()
}

fn build(files: &[(&'static str, String)]) -> (Graph, usize) {
    let db = Database::default();
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let project = support::project_in_path_order(&db, Layout::Project, &borrowed);
    let problems = jadpo_check::check(&db, project);
    (Graph::clone(graph(&db, project)), problems.len())
}

fn document(graph: &Graph, errors: usize) -> String {
    manifest(
        graph,
        &Stamp {
            compiler: "test".into(),
            errors,
        },
    )
    .to_document()
}

/// Each node's key with its digest, leaving out files, whose keys are
/// their paths.
fn identities(graph: &Graph) -> BTreeMap<String, Option<String>> {
    graph
        .nodes()
        .iter()
        .filter(|node| node.kind != Kind::File)
        .map(|node| (node.key.to_string(), node.digest.map(|d| d.short())))
        .collect()
}

#[test]
fn the_fixture_checks_cleanly_and_matches_its_golden_manifest() {
    let (graph, problems) = build(&fixture());
    assert_eq!(problems, 0, "the fixture is a valid project");
    let written = document(&graph, problems);
    let golden =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/manifest.json");
    if std::env::var_os("JADPO_BLESS").is_some() {
        std::fs::create_dir_all(golden.parent().expect("folder")).expect("folder");
        std::fs::write(&golden, &written).expect("write");
    }
    let expected = std::fs::read_to_string(&golden)
        .expect("tests/golden/manifest.json exists; run with JADPO_BLESS=1 to write it");
    assert_eq!(
        written, expected,
        "run with JADPO_BLESS=1 after a deliberate change"
    );
}

#[test]
fn every_node_kind_the_fixture_needs_is_there_with_its_edges() {
    let (graph, _) = build(&fixture());
    let key = |text: &str| Key::from_text(text);
    for expected in [
        "domain notes",
        "file domains/notes/actions/save_note.jadpo",
        "type Body",
        "type Note",
        "field Note.body",
        "type NoteRefused",
        "field NoteRefused.length",
        "library wording",
        "function wording.keep",
        "action save_note",
        "type Shade",
        "variant Shade.plain",
        "variant Shade.tinted",
        "field Shade.tinted.level",
        "field Summary.body",
        "field Summary.shade",
    ] {
        assert!(
            graph.find(&key(expected)).is_some(),
            "{expected} is missing"
        );
    }
    let save = graph.find(&key("action save_note")).expect("action");
    let named = |kind| -> Vec<String> {
        graph
            .outgoing(save, kind)
            .map(|id| graph.node(id).key.to_string())
            .collect()
    };
    assert_eq!(named(EdgeKind::Calls), ["function wording.keep"]);
    assert_eq!(named(EdgeKind::Raises), ["type NoteRefused"]);
    assert_eq!(
        named(EdgeKind::Uses),
        ["type Body", "type Note", "type NoteRefused"]
    );
    let copied = graph.get(&key("field Summary.body")).expect("copied field");
    match &copied.detail {
        jadpo_graph::Detail::Field { copied_from, .. } => {
            assert_eq!(
                copied_from.as_ref().map(Key::as_str),
                Some("field Note.body")
            );
        }
        other => panic!("{other:?}"),
    }
    let failure = graph.get(&key("type NoteRefused")).expect("failure");
    assert_eq!(failure.doc.as_deref(), Some("The note was refused."));
}

#[test]
fn formatting_changes_no_key_and_no_digest() {
    let files = fixture();
    let messy = with(
        &with(
            &with(
                &files,
                REFUSED_PATH,
                // Sections out of the formatter's order, spaced oddly.
                "/// The note was refused.\ntype   NoteRefused = Failure {\n  client: { length: Int }\n\n\n  message: \"The note was refused.\"\n  kind:    FailureKind.rejected\n}\n",
            ),
            NOTE_PATH,
            "use  notes/types {Body,}\ntype Note = Object { body: Body pinned: Bool }\n",
        ),
        WORDING_PATH,
        "library wording {\n/// Keeps a body as it is.\nfunction keep(body: Body,) -> Body { return body }\n}\nuse notes/types { Body }\n",
    );
    let (tidy, _) = build(&files);
    let (untidy, _) = build(&messy);
    assert_eq!(identities(&tidy), identities(&untidy));
    for path in [REFUSED_PATH, NOTE_PATH, WORDING_PATH] {
        let file = |graph: &Graph| {
            graph
                .get(&Key::new(Kind::File, path))
                .and_then(|node| node.digest)
        };
        assert_eq!(file(&tidy), file(&untidy), "{path}");
    }
}

#[test]
fn comments_change_no_digest() {
    let files = fixture();
    let commented = with(
        &files,
        SAVE_PATH,
        &format!(
            "// Saving.\n{}",
            SAVE.replace(
                "    return Note {",
                "    // Build it.\n    return Note { // here\n"
            )
        ),
    );
    assert_eq!(
        identities(&build(&files).0),
        identities(&build(&commented).0)
    );
}

#[test]
fn moves_within_the_layout_keep_every_key_and_digest() {
    let files = fixture();
    // Into the action's own file, then promoted to the domain's types/,
    // then to shared/ (area 1's promotion moves).
    let together = with(
        &without(&files, BODY_PATH),
        SAVE_PATH,
        &format!("{SAVE}\n{BODY}"),
    );
    let shared = {
        let mut out = without(&files, BODY_PATH);
        out.push(("shared/types/body.jadpo", BODY.to_string()));
        out
    };
    let (start, _) = build(&files);
    for moved in [together, shared] {
        let (graph, _) = build(&moved);
        assert_eq!(identities(&start), identities(&graph));
    }
    let (graph, _) = build(&{
        let mut out = without(&files, BODY_PATH);
        out.push(("shared/types/body.jadpo", BODY.to_string()));
        out
    });
    let body = graph.get(&Key::new(Kind::Type, "Body")).expect("Body");
    assert_eq!(body.domain, None, "shared/ is owned by no domain");
    assert!(
        diff(&start, &graph)
            .changes
            .iter()
            .all(|change| change.kind == Kind::File)
    );
}

#[test]
fn an_edit_changes_only_the_digests_of_what_it_touches() {
    let files = fixture();
    let edited = with(&files, SAVE_PATH, &SAVE.replace("length: 2", "length: 3"));
    let (before, _) = build(&files);
    let (after, _) = build(&edited);
    let changes = diff(&before, &after);
    let changed: Vec<&str> = changes.changes.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(
        changed,
        [
            "action save_note",
            "file domains/notes/actions/save_note.jadpo"
        ]
    );
    let What::Changed(what) = &changes.changes[0].what else {
        panic!("changed")
    };
    assert!(what.digest && !what.detail && what.edges.is_empty());
}

/// Files read in any order give the same bytes (they are always listed in
/// path order, as `jadpo_db::load` lists them).
#[test]
fn a_function_that_calls_itself_keeps_the_edge() {
    let files = with(
        &fixture(),
        "domains/notes/libraries/counting.jadpo",
        "library counting {\n    function down(body: Body) -> Body {\n        if (body == Body(\"x\")) {\n            return body\n        }\n        return down(Body(\"x\"))\n    }\n}\n",
    );
    let (graph, _) = build(&files);
    let down = graph
        .find(&Key::from_text("function counting.down"))
        .expect("function");
    assert_eq!(
        graph.outgoing(down, EdgeKind::Calls).collect::<Vec<_>>(),
        [down]
    );
}

#[test]
fn a_type_written_as_a_field_uses_that_field_only() {
    let files = with(
        &fixture(),
        "domains/notes/actions/echo_body.jadpo",
        "action echo_body(text: Note.body) -> Note.body {\n    return text\n}\n",
    );
    let (graph, _) = build(&files);
    let echo = graph
        .find(&Key::from_text("action echo_body"))
        .expect("action");
    let uses: Vec<String> = graph
        .outgoing(echo, EdgeKind::Uses)
        .map(|id| graph.node(id).key.to_string())
        .collect();
    assert_eq!(uses, ["field Note.body"]);
}

#[test]
fn the_manifest_is_the_same_bytes_in_any_file_order() {
    let files = fixture();
    let (graph, errors) = build(&files);
    let mut reversed = files.clone();
    reversed.reverse();
    let (again, again_errors) = build(&reversed);
    assert_eq!(document(&graph, errors), document(&again, again_errors));
    let mut rotated = files.clone();
    rotated.rotate_left(3);
    let (third, third_errors) = build(&rotated);
    assert_eq!(document(&graph, errors), document(&third, third_errors));
}

#[test]
fn a_diff_names_what_was_added_and_what_needs_a_person() {
    let files = fixture();
    let grown = {
        let mut out = files.clone();
        out.push((
            "domains/archive/actions/store_note.jadpo",
            "action store_note(body: Body) -> Body {\n    return wording.keep(body)\n}\n"
                .to_string(),
        ));
        out
    };
    let (before, _) = build(&files);
    // The new domain reaches into another's library, which the checker
    // reports; the graph still holds what it could resolve.
    let (after, errors) = build(&grown);
    assert!(errors > 0);
    let changes = diff(&before, &after);
    let added: Vec<(&str, &str)> = changes
        .changes
        .iter()
        .map(|c| (c.key.as_str(), c.what.word()))
        .collect();
    assert_eq!(
        added,
        [
            ("action store_note", "added"),
            ("domain archive", "added"),
            ("file domains/archive/actions/store_note.jadpo", "added"),
        ]
    );
    assert_eq!(
        changes.get("domain archive").expect("domain").needs,
        ["DomainChange.created"]
    );
    let json = changes.to_json().to_document();
    assert!(
        json.contains("\"needs\": [\n        \"DomainChange.created\"\n      ]"),
        "{json}"
    );
}

fn executed(db: &Database, query: &str) -> usize {
    db.take_executions()
        .iter()
        .filter(|key| key.starts_with(&format!("{query}(")))
        .count()
}

#[test]
fn editing_a_body_rebuilds_only_that_files_part() {
    let mut db = Database::recording();
    let files = fixture();
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let project = support::project(&db, Layout::Project, &borrowed);
    graph(&db, project);
    assert_eq!(executed(&db, "file_part"), files.len());
    let save = project
        .files(&db)
        .iter()
        .copied()
        .find(|file| file.path(&db) == SAVE_PATH)
        .expect("file");
    save.set_text(&mut db)
        .to(SAVE.replace("length: 2", "length: 3"));
    graph(&db, project);
    let log = db.take_executions();
    let count = |query: &str| {
        log.iter()
            .filter(|key| key.starts_with(&format!("{query}(")))
            .count()
    };
    assert_eq!(count("file_part"), 1, "only the edited file: {log:?}");
    assert_eq!(count("type_table"), 0, "{log:?}");
    assert_eq!(count("graph"), 1, "assembled again: {log:?}");
}

/// The identity registry is an input (area 18): a rename it records
/// re-keys the renamed node and what refers to it, and nothing else.
#[test]
fn a_rename_the_registry_records_rekeys_what_it_touches() {
    let mut db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[(
            "main.jadpo",
            "type Count = Int {\n    min: 0\n}\n\nfunction double(x: Count) -> Count {\n    return x\n}\n",
        )],
    );
    let before = jadpo_check::graph::graph(&db, project).clone();
    assert!(before.get(&Key::new(Kind::Type, "Count")).is_some());

    project
        .set_identities(&mut db)
        .to([("type Count".to_string(), "Tally".to_string())].into());
    let after = jadpo_check::graph::graph(&db, project);
    let tally = after
        .get(&Key::new(Kind::Type, "Tally"))
        .expect("the type keeps its born name");
    assert_eq!(tally.name, "Count", "and its name now");
    let double = after
        .find(&Key::new(Kind::Function, "double"))
        .expect("double");
    let uses: Vec<String> = after
        .outgoing(double, EdgeKind::Uses)
        .map(|id| after.node(id).key.to_string())
        .collect();
    assert_eq!(uses, ["type Tally"]);
}
