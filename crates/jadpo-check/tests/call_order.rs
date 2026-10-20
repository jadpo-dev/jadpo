//! Writes before outside calls (area 6, "Several outside calls"; chunk
//! 7.4b): no transaction is open while an outside call is out, so an
//! action's writes come after its last call, and a write placed before a
//! later call is `data.write_before_call`, reported at the write and
//! naming the call it must follow. Each way through an `if` or a `match`
//! counts on its own, a `return` ends its way, a loop's write meets a call
//! after the loop, and an action called counts for the calls and writes it
//! makes, itself or through the actions it calls. A run's `each:` and
//! `finished:` are the run's, never the starting action's; starting a run
//! writes its row.
//!
//! The same reading tells the plan which actions make outside calls
//! ([`outside_calls`]): an item of a run whose `each:` action does is a
//! batch of its own (area 12, "How a run goes").

use crate::support;

use jadpo_check::types::outside_calls;
use jadpo_db::{Database, Layout};

const APP: (&str, &str) = (
    "app.jadpo",
    "application {\n    connections: {\n        sheets: {\n            driver: Driver.disk\n            path: \"/var/lib/pantry/sheets\"\n        }\n    }\n    steps: {\n        encryption_key: config.step_encryption_key\n    }\n}\n",
);

const BASE: &[(&str, &str)] = &[
    (
        "config.jadpo",
        "config {\n    step_encryption_key: SigningKey\n}\n",
    ),
    (
        "domains/pantry/types/note.jadpo",
        "type Note = Text {\n    max_length: 40\n}\n",
    ),
    (
        "domains/pantry/types/sheet_text.jadpo",
        "type SheetText = Text {\n    max_length: 100000\n}\n",
    ),
    (
        "domains/pantry/types/shelf_page.jadpo",
        "type ShelfPage = Input {\n    page_size: PageSize {\n        default: 25\n    }\n    after: PageCursor {\n        optional: true\n    }\n}\n",
    ),
    (
        "domains/pantry/failures/no_room.jadpo",
        "type NoRoom = Failure {\n    kind: FailureKind.unavailable\n}\n",
    ),
    (
        "domains/pantry/entities/shelf.jadpo",
        "use pantry/types { Note }\n\nentity Shelf {\n    id: Uuid\n    note: Note\n    sheet: File? {\n        store: sheets\n        max_size: 5mb\n        types: [FileType.csv]\n    }\n\n    policy: {\n        create: [Access.public, Access.system]\n        read: [Access.public, Access.system]\n        update: [Access.public, Access.system]\n    }\n}\n",
    ),
    (
        "domains/pantry/queries/shelves.jadpo",
        "use pantry/entities { Shelf }\nuse pantry/types { ShelfPage }\n\nquery shelves(page: ShelfPage) -> Page<Shelf> {\n    from: Shelf\n    order: [Shelf.id asc]\n    page: {\n        size: page.page_size\n        after: page.after\n    }\n}\n",
    ),
    // Writes only.
    (
        "domains/pantry/actions/add_shelf.jadpo",
        "use pantry/entities { Shelf }\nuse pantry/types { Note }\n\naction add_shelf(note: Note) -> Shelf {\n    return Shelf.create {\n        set: {\n            note: note\n            sheet: none\n        }\n    }\n}\n",
    ),
    // Calls only.
    (
        "domains/pantry/actions/put_sheet.jadpo",
        "use app { sheets }\nuse pantry/failures { NoRoom }\nuse pantry/types { SheetText }\n\naction put_sheet(sheet_text: SheetText) fails NoRoom -> File {\n    return attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {\n        storage_full: NoRoom\n    }\n}\n",
    ),
    // Calls through an action it calls.
    (
        "domains/pantry/actions/put_twice.jadpo",
        "use pantry/actions { put_sheet }\nuse pantry/failures { NoRoom }\nuse pantry/types { SheetText }\n\naction put_twice(sheet_text: SheetText) fails NoRoom -> File {\n    var first: File = attempt put_sheet(sheet_text)\n    assert first == first\n    return attempt put_sheet(sheet_text)\n}\n",
    ),
    // Calls, then writes the shelf it was handed: what a run's item does.
    (
        "domains/pantry/actions/file_shelf.jadpo",
        "use app { sheets }\nuse pantry/entities { Shelf }\nuse pantry/failures { NoRoom }\n\naction file_shelf(shelf: Shelf) fails NoRoom -> Unit {\n    var sheet: File = attempt sheets.put(encoding.to_utf8(shelf.note), FileType.csv) {\n        storage_full: NoRoom\n    }\n    var filed: Shelf = Shelf.update(shelf) {\n        set: {\n            sheet: sheet\n        }\n    }\n    assert filed.sheet == sheet\n}\n",
    ),
];

/// The imports every `keep` may use; one it doesn't use isn't reported.
const IMPORTS: &str = "use app { sheets }\nuse pantry/actions { add_shelf, file_shelf, put_sheet, put_twice }\nuse pantry/entities { Shelf }\nuse pantry/failures { NoRoom }\nuse pantry/queries { shelves }\nuse pantry/types { Note, SheetText, ShelfPage }\n\n";

fn files(keep: &str) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = std::iter::once(APP)
        .chain(BASE.iter().copied())
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect();
    files.push((
        "domains/pantry/actions/keep.jadpo".to_string(),
        format!("{IMPORTS}{keep}"),
    ));
    files
}

/// Each problem `keep` gives, as `rule: message`, but for unused imports,
/// declarations and variables and migrations, which these projects don't
/// keep.
fn problems(keep: &str) -> Vec<String> {
    let files = files(keep);
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| {
            ![
                "import.unused",
                "layout.unused_declaration",
                "semantic.unused_variable",
            ]
            .iter()
            .any(|rule| problem.contains(&format!(" {rule}:")))
                && !problem.contains(" migration.")
        })
        .collect()
}

/// The diagnostics `keep` gives under `data.write_before_call`.
fn found(keep: &str) -> Vec<jadpo_diag::Diagnostic> {
    let files = files(keep);
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    jadpo_check::check(&db, project)
        .into_iter()
        .filter(|diagnostic| diagnostic.rule.id == "data.write_before_call")
        .collect()
}

const KEEP: &str = "domains/pantry/actions/keep.jadpo";

/// The text a label points at in `keep`'s file.
fn pointed(keep: &str, label: &jadpo_diag::Label) -> String {
    let text = format!("{IMPORTS}{keep}");
    text[label.span.start as usize..label.span.end as usize].to_string()
}

#[test]
fn a_call_before_the_actions_writes_checks() {
    assert_eq!(
        problems(
            "action keep(note: Note, sheet_text: SheetText) fails NoRoom -> Shelf {\n    var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {\n        storage_full: NoRoom\n    }\n    var shelf: Shelf = Shelf.create {\n        set: {\n            note: note\n            sheet: sheet\n        }\n    }\n    var kept: Shelf = Shelf.update(shelf) {\n        set: {\n            note: note\n        }\n    }\n    return kept\n}\n"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_write_before_a_call_is_refused_at_the_write_naming_the_call() {
    let keep = "action keep(note: Note, sheet_text: SheetText) fails NoRoom -> Shelf {\n    var shelf: Shelf = Shelf.create {\n        set: {\n            note: note\n            sheet: none\n        }\n    }\n    var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {\n        storage_full: NoRoom\n    }\n    assert sheet == sheet\n    return shelf\n}\n";
    assert_eq!(
        problems(keep),
        [format!(
            "{KEEP} data.write_before_call: `Shelf.create` writes before the call to `sheets.put`"
        )]
    );
    let [diagnostic] = found(keep).try_into().expect("one");
    let primary = diagnostic.primary.as_ref().expect("at the write");
    assert_eq!(pointed(keep, primary), "Shelf.create");
    assert_eq!(primary.text, "writes here");
    let [call] = diagnostic.secondary.as_slice() else {
        panic!("{diagnostic:?}");
    };
    assert_eq!(pointed(keep, call), "sheets.put");
    assert_eq!(call.text, "the call it must follow");
    assert_eq!(
        diagnostic.next.as_ref().map(|next| next.text.as_str()),
        Some("Move `Shelf.create` after `sheets.put`")
    );
    assert!(
        !diagnostic.rule.declinable,
        "a plain mistake, never declined"
    );
}

#[test]
fn a_write_on_another_way_or_ended_by_a_return_meets_no_call() {
    for keep in [
        // The write on one way, the call on the other.
        "action keep(note: Note, sheet_text: SheetText) fails NoRoom -> Unit {\n    if (note == \"\") {\n        var shelf: Shelf = add_shelf(note)\n    } else {\n        var sheet: File = attempt put_sheet(sheet_text)\n    }\n}\n",
        // A `match` the same.
        "action keep(note: Note, sheet_text: SheetText) fails NoRoom -> Unit {\n    var empty: Bool = note == \"\"\n    match empty {\n        true -> {\n            var shelf: Shelf = add_shelf(note)\n        }\n        false -> {\n            var sheet: File = attempt put_sheet(sheet_text)\n        }\n    }\n}\n",
        // The write's way ends before the call.
        "action keep(note: Note, sheet_text: SheetText) fails NoRoom -> Unit {\n    if (note == \"\") {\n        var shelf: Shelf = add_shelf(note)\n        return\n    }\n    var sheet: File = attempt put_sheet(sheet_text)\n}\n",
    ] {
        assert_eq!(problems(keep), Vec::<String>::new(), "{keep}");
    }
}

#[test]
fn a_write_or_a_call_through_an_action_counts_as_the_actions() {
    // An action that writes, before a call.
    assert_eq!(
        problems(
            "action keep(note: Note, sheet_text: SheetText) fails NoRoom -> Shelf {\n    var shelf: Shelf = add_shelf(note)\n    var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {\n        storage_full: NoRoom\n    }\n    assert sheet == sheet\n    return shelf\n}\n"
        ),
        [format!(
            "{KEEP} data.write_before_call: `add_shelf` writes before the call to `sheets.put`"
        )]
    );
    // A write before an action that calls, through another it calls.
    let keep = "action keep(note: Note, sheet_text: SheetText) fails NoRoom -> Shelf {\n    var shelf: Shelf = Shelf.create {\n        set: {\n            note: note\n            sheet: none\n        }\n    }\n    var sheet: File = attempt put_twice(sheet_text)\n    assert sheet == sheet\n    return shelf\n}\n";
    assert_eq!(
        problems(keep),
        [format!(
            "{KEEP} data.write_before_call: `Shelf.create` writes before `put_twice` calls `sheets.put`"
        )]
    );
    let [diagnostic] = found(keep).try_into().expect("one");
    let [call] = diagnostic.secondary.as_slice() else {
        panic!("{diagnostic:?}");
    };
    assert_eq!(pointed(keep, call), "put_twice");
    assert_eq!(call.text, "calls `sheets.put`");
    assert_eq!(
        diagnostic.next.as_ref().map(|next| next.text.as_str()),
        Some("Move `Shelf.create` after the call to `put_twice`")
    );
    // An action that calls then writes, called on its own, is in order.
    assert_eq!(
        problems(
            "action keep(shelf: Shelf, note: Note) fails NoRoom -> Shelf {\n    attempt file_shelf(shelf)\n    return add_shelf(note)\n}\n"
        ),
        Vec::<String>::new()
    );
    // Its write comes before the next call.
    assert_eq!(
        problems(
            "action keep(shelf: Shelf, sheet_text: SheetText) fails NoRoom -> File {\n    attempt file_shelf(shelf)\n    return attempt put_sheet(sheet_text)\n}\n"
        ),
        [format!(
            "{KEEP} data.write_before_call: `file_shelf` writes before `put_sheet` calls `sheets.put`"
        )]
    );
}

#[test]
fn a_write_in_a_loop_meets_a_call_after_it() {
    assert_eq!(
        problems(
            "action keep(notes: List<Note>, sheet_text: SheetText) fails NoRoom -> File {\n    for note in notes {\n        var shelf: Shelf = add_shelf(note)\n    }\n    return attempt put_sheet(sheet_text)\n}\n"
        ),
        [format!(
            "{KEEP} data.write_before_call: `add_shelf` writes before `put_sheet` calls `sheets.put`"
        )]
    );
}

#[test]
fn a_write_before_several_calls_names_the_last_and_is_reported_once() {
    let keep = "action keep(note: Note, sheet_text: SheetText) fails NoRoom -> Shelf {\n    var shelf: Shelf = add_shelf(note)\n    var first: File = attempt put_sheet(sheet_text)\n    var second: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {\n        storage_full: NoRoom\n    }\n    assert first == second\n    return shelf\n}\n";
    assert_eq!(
        problems(keep),
        [format!(
            "{KEEP} data.write_before_call: `add_shelf` writes before the call to `sheets.put`"
        )]
    );
}

#[test]
fn starting_a_run_writes_and_its_items_calls_are_the_runs() {
    // `each:`'s action calls, but the run makes those calls, not the
    // action starting it.
    assert_eq!(
        problems(
            "action keep(page: ShelfPage, note: Note) -> BatchRun {\n    var kept: Shelf = add_shelf(note)\n    return batches.each(\n        from: shelves(page),\n        each: shelf -> file_shelf(shelf),\n        key: kept.id\n    )\n}\n"
        ),
        Vec::<String>::new()
    );
    // Starting it keeps its row, a write before the call after it.
    assert_eq!(
        problems(
            "action keep(page: ShelfPage, sheet_text: SheetText) fails NoRoom -> BatchRun {\n    var run: BatchRun = batches.each(\n        from: shelves(page),\n        each: shelf -> file_shelf(shelf)\n    )\n    var sheet: File = attempt put_sheet(sheet_text)\n    assert sheet == sheet\n    return run\n}\n"
        ),
        [format!(
            "{KEEP} data.write_before_call: `batches.each` writes before `put_sheet` calls `sheets.put`"
        )]
    );
}

#[test]
fn which_actions_make_outside_calls_and_which_write() {
    let files = files("action keep(note: Note) -> Shelf {\n    return add_shelf(note)\n}\n");
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let found: Vec<(String, Option<String>, Option<String>)> = outside_calls(&db, project)
        .iter()
        .map(|(key, outside)| {
            (
                key.name().to_string(),
                outside.calls.clone(),
                outside.writes.clone(),
            )
        })
        .collect();
    let named = |name: &str| {
        found
            .iter()
            .find(|(found, _, _)| found == name)
            .map(|(_, calls, writes)| (calls.as_deref(), writes.as_deref()))
    };
    assert_eq!(named("add_shelf"), Some((None, Some("Shelf.create"))));
    assert_eq!(named("put_sheet"), Some((Some("sheets.put"), None)));
    assert_eq!(named("put_twice"), Some((Some("sheets.put"), None)));
    assert_eq!(
        named("file_shelf"),
        Some((Some("sheets.put"), Some("Shelf.update")))
    );
    assert_eq!(named("keep"), Some((None, Some("Shelf.create"))));
}
