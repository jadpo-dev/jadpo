//! Isolation by what an action does (area 6, "Counters and other one-step
//! updates", chunk 3.7): an action that loads no record, itself or through
//! any action it calls, runs at read committed; every other action at
//! repeatable read. Each way of loading a record, and each way of not, on
//! a project of its own; the decision on the graph's nodes and in the
//! manifest; and every action and fixture of the teaching examples that
//! keep records, against a golden list (`JADPO_BLESS=1` rewrites it).

mod support;

use std::collections::BTreeMap;
use std::path::Path;

use jadpo_check::graph::graph;
use jadpo_check::types::{Decided, Loads, isolation};
use jadpo_db::{Database, Layout};
use jadpo_graph::manifest::{Stamp, manifest};
use jadpo_graph::{Key, Kind};
use jadpo_std::Isolation;

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/shop/types/count.jadpo",
        "type Count = Int {\n    min: 0\n}\n",
    ),
    (
        "domains/shop/types/stage.jadpo",
        "type Stage = Enum {\n    open\n    closed\n}\n",
    ),
    (
        "domains/shop/types/holder.jadpo",
        "use shop/entities { Shelf }\n\ntype Holder = Object {\n    shelves: List<Shelf>\n}\n",
    ),
    (
        "domains/shop/entities/shelf.jadpo",
        "use shop/types { Count, Stage }\n\nentity Shelf {\n    id: Uuid\n    visits: Count\n    stage: Stage\n    removed_at: Instant?\n\n    lifecycle: {\n        visible: removed_at == none\n        initial: {\n            stage: Stage.open\n        }\n        transitions: {\n            close: {\n                when: stage == Stage.open\n                set: {\n                    stage: Stage.closed\n                }\n            }\n            delete: {\n                when: removed_at == none\n                set: {\n                    removed_at: clock.now\n                }\n            }\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/failures/shelf_closed.jadpo",
        "type ShelfClosed = Failure {\n    kind: FailureKind.conflict\n}\n",
    ),
    (
        "domains/shop/queries/open_shelves.jadpo",
        "use shop/entities { Shelf }\n\nquery open_shelves() -> List<Shelf> {\n    from: Shelf\n    order: [Shelf.visits desc]\n    limit: 10\n}\n",
    ),
    // Read committed: a create, a counter, a delete by `id` running an
    // unguarded transition, and actions calling only those.
    (
        "domains/shop/actions/add_shelf.jadpo",
        "use shop/entities { Shelf }\n\naction add_shelf() -> Shelf {\n    return Shelf.create {\n        set: {\n            visits: 0\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/visit.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\nuse shop/types { Count }\n\naction visit(shelf_id: Shelf.id) fails ShelfNotFound -> Shelf {\n    return attempt Shelf.update(shelf_id) {\n        set: {\n            visits: Count(Shelf.visits + 1)\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/remove_shelf.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\n\naction remove_shelf(shelf_id: Shelf.id) fails ShelfNotFound -> Unit {\n    attempt Shelf.delete(shelf_id)\n}\n",
    ),
    (
        "domains/shop/actions/visit_twice.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\nuse shop/actions { visit }\n\naction visit_twice(shelf_id: Shelf.id) fails ShelfNotFound -> Shelf {\n    attempt visit(shelf_id)\n    return attempt visit(shelf_id)\n}\n",
    ),
    // Repeatable read, one way each.
    (
        "domains/shop/actions/tally.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\nuse shop/types { Count }\n\naction tally(shelf_id: Shelf.id) fails ShelfNotFound -> Shelf {\n    var shelf: Shelf = attempt Shelf.get(shelf_id)\n    return attempt Shelf.update(shelf.id) {\n        set: {\n            visits: Count(Shelf.visits + 1)\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/busiest.jadpo",
        "use shop/entities { Shelf }\nuse shop/queries { open_shelves }\n\naction busiest() -> List<Shelf> {\n    return open_shelves()\n}\n",
    ),
    (
        "domains/shop/actions/count_handed.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\nuse shop/types { Count }\n\naction count_handed(shelf: Shelf) fails ShelfNotFound -> Shelf {\n    return attempt Shelf.update(shelf.id) {\n        set: {\n            visits: Count(Shelf.visits + 1)\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/count_held.jadpo",
        "use shop/entities { Shelf }\nuse shop/types { Holder }\n\naction count_held(holder: Holder) -> List<Shelf> {\n    return holder.shelves\n}\n",
    ),
    (
        "domains/shop/actions/reopen_new.jadpo",
        "use shop/entities { Shelf }\nuse shop/actions { add_shelf }\n\naction reopen_new() -> Unit {\n    var shelf: Shelf = add_shelf()\n    Shelf.delete(shelf)\n}\n",
    ),
    (
        "domains/shop/actions/close_shelf.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\nuse shop/failures { ShelfClosed }\n\naction close_shelf(shelf_id: Shelf.id) fails ShelfNotFound, ShelfClosed -> Shelf {\n    return attempt Shelf.close(shelf_id) {\n        conflict: {\n            Shelf.close: ShelfClosed\n        }\n    }\n}\n",
    ),
    // Through calls: directly, and through an action that loads nothing
    // itself.
    (
        "domains/shop/actions/recount.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\nuse shop/actions { tally }\n\naction recount(shelf_id: Shelf.id) fails ShelfNotFound -> Shelf {\n    return attempt tally(shelf_id)\n}\n",
    ),
    (
        "domains/shop/actions/visit_then_recount.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\nuse shop/actions { recount, visit }\n\naction visit_then_recount(shelf_id: Shelf.id) fails ShelfNotFound -> Shelf {\n    attempt visit(shelf_id)\n    return attempt recount(shelf_id)\n}\n",
    ),
    (
        "domains/shop/actions/start_day.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\nuse shop/actions { visit_then_recount }\n\naction start_day(shelf_id: Shelf.id) fails ShelfNotFound -> Unit {\n    attempt visit_then_recount(shelf_id)\n}\n",
    ),
    (
        "domains/shop/fixtures/busy_shelf.jadpo",
        "use shop/entities { Shelf }\nuse shop/actions { add_shelf, tally }\n\nfixture busy_shelf() -> Shelf {\n    var shelf: Shelf = add_shelf()\n    return attempt tally(shelf.id)\n}\n",
    ),
    (
        "domains/shop/fixtures/new_shelf.jadpo",
        "use shop/entities { Shelf }\nuse shop/actions { add_shelf }\n\nfixture new_shelf() -> Shelf {\n    return add_shelf()\n}\n",
    ),
];

fn decided(db: &Database, files: &[(&str, &str)]) -> BTreeMap<String, Decided> {
    let project = support::project(db, Layout::Project, files);
    let problems = jadpo_check::check(db, project);
    assert!(
        problems.is_empty(),
        "{:?}",
        problems
            .iter()
            .map(|d| format!("{}: {}", d.rule.id, d.message))
            .collect::<Vec<_>>()
    );
    isolation(db, project)
        .iter()
        .map(|(key, decided)| (key.as_str().to_string(), decided.clone()))
        .collect()
}

fn at(isolation: Isolation, because: Option<Loads>) -> Decided {
    Decided { isolation, because }
}

#[test]
fn each_way_of_loading_a_record_and_of_not() {
    let db = Database::default();
    let found = decided(&db, FILES);
    let read_committed = at(Isolation::ReadCommitted, None);
    let rr = |because: Loads| at(Isolation::RepeatableRead, Some(because));
    let expected: BTreeMap<String, Decided> = [
        // A create; a counter by `id`; a delete by `id` that runs an
        // unguarded transition (its `when:` asks only what `visible:`
        // does); an action calling only those; a fixture calling one.
        ("action add_shelf", read_committed.clone()),
        ("action visit", read_committed.clone()),
        ("action remove_shelf", read_committed.clone()),
        ("action visit_twice", read_committed.clone()),
        ("fixture new_shelf", read_committed.clone()),
        // `get`, a query, a record as a parameter, one inside an object's
        // list, a record written back, and a transition by `id` whose
        // `when:` can refuse a visible record.
        ("action tally", rr(Loads::Gets("Shelf".into()))),
        ("action busiest", rr(Loads::Queries("open_shelves".into()))),
        ("action count_handed", rr(Loads::Handed("shelf".into()))),
        ("action count_held", rr(Loads::Handed("holder".into()))),
        ("action reopen_new", rr(Loads::WritesBack("Shelf".into()))),
        (
            "action close_shelf",
            rr(Loads::Guarded("Shelf.close".into())),
        ),
        // Through the actions and fixtures it calls, however deep, named
        // by the one it calls directly.
        ("action recount", rr(Loads::Calls("tally".into()))),
        (
            "action visit_then_recount",
            rr(Loads::Calls("recount".into())),
        ),
        (
            "action start_day",
            rr(Loads::Calls("visit_then_recount".into())),
        ),
        ("fixture busy_shelf", rr(Loads::Calls("tally".into()))),
    ]
    .into_iter()
    .map(|(key, decided)| (key.to_string(), decided))
    .collect();
    assert_eq!(found, expected);
}

#[test]
fn an_action_starts_loading_when_an_action_it_calls_does() {
    let db = Database::default();
    let mut files: Vec<(&str, &str)> = FILES.to_vec();
    let before = decided(&db, &files);
    assert_eq!(
        before["action visit_twice"].isolation,
        Isolation::ReadCommitted
    );
    // `visit` reads the shelf first now: every action that calls it, and
    // every one that calls those, runs at repeatable read.
    let reading = "use shop/entities { Shelf, ShelfNotFound }\nuse shop/types { Count }\n\naction visit(shelf_id: Shelf.id) fails ShelfNotFound -> Shelf {\n    var shelf: Shelf = attempt Shelf.get(shelf_id)\n    return attempt Shelf.update(shelf.id) {\n        set: {\n            visits: Count(Shelf.visits + 1)\n        }\n    }\n}\n";
    for file in &mut files {
        if file.0 == "domains/shop/actions/visit.jadpo" {
            file.1 = reading;
        }
    }
    let after = decided(&db, &files);
    assert_eq!(
        after["action visit_twice"],
        at(
            Isolation::RepeatableRead,
            Some(Loads::Calls("visit".into()))
        )
    );
    assert_eq!(after["action visit"].isolation, Isolation::RepeatableRead);
    assert_eq!(
        after["action add_shelf"].isolation,
        Isolation::ReadCommitted,
        "nothing else changed"
    );
}

#[test]
fn the_graph_and_the_manifest_carry_each_actions_and_fixtures_level() {
    let db = Database::default();
    let project = support::project(&db, Layout::Project, FILES);
    let graph = graph(&db, project);
    let level = |kind: Kind, name: &str| {
        graph
            .get(&Key::new(kind, name))
            .unwrap_or_else(|| panic!("{name}"))
            .isolation()
    };
    assert_eq!(level(Kind::Action, "visit"), Some(Isolation::ReadCommitted));
    assert_eq!(
        level(Kind::Action, "recount"),
        Some(Isolation::RepeatableRead)
    );
    assert_eq!(
        level(Kind::Fixture, "busy_shelf"),
        Some(Isolation::RepeatableRead)
    );
    assert_eq!(
        level(Kind::Query, "open_shelves"),
        None,
        "a query begins no transaction"
    );
    let written = manifest(
        &graph,
        &Stamp {
            compiler: "test".into(),
            errors: 0,
        },
    )
    .to_document();
    assert!(
        written.contains("\"isolation\": \"read_committed\""),
        "{written}"
    );
    assert!(
        written.contains("\"isolation\": \"repeatable_read\""),
        "{written}"
    );
}

// --- the teaching examples ---------------------------------------------------

/// Every action and fixture in each example that keeps records, with its
/// level and why it loads a record when it does.
fn examples() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut projects: Vec<_> = std::fs::read_dir(&root)
        .expect("examples/")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.join("app.jadpo").is_file())
        .collect();
    projects.sort();
    let mut out = String::new();
    for dir in projects {
        let loaded = jadpo_db::load(&dir);
        let (db, project) = (&loaded.db, loaded.project);
        if jadpo_check::data::schema(db, project).stores.is_empty() {
            continue;
        }
        let name = dir.file_name().expect("a name").to_string_lossy();
        out.push_str(&format!("{name}\n"));
        for (key, decided) in isolation(db, project) {
            let why = decided
                .because
                .as_ref()
                .map(|because| format!(": {}", because.describe()))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {:<34} {}{why}\n",
                key.as_str(),
                decided.isolation.word()
            ));
        }
    }
    out
}

#[test]
fn the_teaching_examples_actions_run_at_the_levels_listed() {
    let found = examples();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/isolation.txt");
    if std::env::var_os("JADPO_BLESS").is_some() {
        std::fs::write(&path, &found).expect("write the golden list");
    }
    let expected = std::fs::read_to_string(&path)
        .expect("tests/golden/isolation.txt exists; run with JADPO_BLESS=1 to write it");
    assert_eq!(
        found, expected,
        "run with JADPO_BLESS=1 after a deliberate change"
    );
}
