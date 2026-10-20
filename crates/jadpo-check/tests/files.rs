//! Files and the file helpers' reads in the checker (area 16, "Files";
//! area 19, "Data formats"; area 12, "How a file's run is kept"; chunk
//! 6.4b): a `File` field's `store:`, `max_size:` and `types:`; a store's
//! `put`, whose `storage_full` only an `unavailable` failure maps, and its
//! `read`, held to 10 MB; both only in an action, and neither built by
//! hand; and `csv.read`, `tsv.read` and `json_lines.read`, only in an
//! action, given a stored `File` and an `Input` each row is read as, a CSV
//! or TSV row holding one value a cell and a column read once.

use crate::support;

use jadpo_db::Layout;

const APP: (&str, &str) = (
    "app.jadpo",
    "application {\n    connections: {\n        sheets: {\n            driver: Driver.disk\n            path: \"/var/lib/pantry/sheets\"\n        }\n    }\n}\n",
);

const BASE: &[(&str, &str)] = &[
    (
        "domains/pantry/types/shelf_code.jadpo",
        "type ShelfCode = Text {\n    min_length: 1\n    max_length: 20\n}\n",
    ),
    (
        "domains/pantry/types/sheet_text.jadpo",
        "type SheetText = Text {\n    max_length: 100000\n}\n",
    ),
    (
        "domains/pantry/types/shelf_row.jadpo",
        "use pantry/types { ShelfCode }\n\ntype ShelfRow = Input {\n    code: ShelfCode {\n        column: \"Shelf code\"\n    }\n}\n",
    ),
    (
        "domains/pantry/failures/no_room.jadpo",
        "type NoRoom = Failure {\n    kind: FailureKind.unavailable\n}\n",
    ),
    (
        "domains/pantry/failures/bad_sheet.jadpo",
        "type BadSheet = Failure {\n    kind: FailureKind.invalid_value\n}\n",
    ),
    (
        "domains/pantry/entities/shelf_sheet.jadpo",
        "entity ShelfSheet {\n    id: Uuid\n    sheet: File {\n        store: sheets\n        max_size: 5mb\n        types: [FileType.csv]\n    }\n\n    policy: {\n        create: [Access.public, Access.system]\n        read: [Access.public, Access.system]\n    }\n}\n",
    ),
    (
        "domains/pantry/actions/note_shelf.jadpo",
        "use pantry/types { ShelfRow }\n\naction note_shelf(listed: ShelfRow) -> Unit {\n    assert listed.code == listed.code\n}\n",
    ),
];

/// What the base project, with `extra` files besides, reports, but for
/// unused imports and declarations and migrations, which these projects
/// don't keep.
fn ids(extra: &[(&str, &str)]) -> Vec<String> {
    let mut files: Vec<(&str, &str)> = vec![APP];
    files.extend_from_slice(BASE);
    files.extend_from_slice(extra);
    support::rules(Layout::Project, &files)
        .into_iter()
        .filter(|rule| {
            rule != "import.unused"
                && rule != "layout.unused_declaration"
                && !rule.starts_with("migration.")
        })
        .collect()
}

fn action(body: &str) -> String {
    format!(
        "use app {{ sheets }}\nuse pantry/entities {{ ShelfSheet, ShelfSheetNotFound }}\nuse pantry/types {{ SheetText, ShelfRow }}\nuse pantry/failures {{ BadSheet, NoRoom }}\nuse pantry/actions {{ note_shelf }}\n\n{body}"
    )
}

#[test]
fn a_put_kept_and_a_file_read_check() {
    let good = action(
        "action keep(sheet_text: SheetText) fails NoRoom -> ShelfSheet {\n    var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {\n        storage_full: NoRoom\n    }\n    var whole: FileContents = sheets.read(sheet)\n    var read: BatchRun = csv.read(sheet, as: ShelfRow, each: listed -> note_shelf(listed))\n    return ShelfSheet.create {\n        set: {\n            sheet: sheet\n        }\n    }\n}\n",
    );
    let found = ids(&[("domains/pantry/actions/keep.jadpo", &good)]);
    assert!(
        found.iter().all(|rule| rule == "semantic.unused_variable"),
        "{found:?}"
    );
}

#[test]
fn a_file_fields_settings_are_held_to_what_they_say() {
    let field = |settings: &str| {
        format!(
            "entity Spare {{\n    id: Uuid\n    sheet: File {{\n{settings}    }}\n\n    policy: {{\n        create: [Access.public]\n        read: [Access.public]\n    }}\n}}\n"
        )
    };
    for (settings, expected) in [
        ("        max_size: 5mb\n", "data.file_field_invalid"),
        (
            "        store: main\n        max_size: 5mb\n",
            "data.file_field_invalid",
        ),
        (
            "        store: elsewhere\n        max_size: 5mb\n",
            "data.file_field_invalid",
        ),
        ("        store: sheets\n", "data.file_field_invalid"),
        (
            "        store: sheets\n        max_size: 5\n",
            "data.file_field_invalid",
        ),
        (
            "        store: sheets\n        max_size: 5tb\n",
            "data.file_field_invalid",
        ),
        (
            "        store: sheets\n        max_size: 5mb\n        types: [FileType.mp3]\n",
            "data.file_field_invalid",
        ),
    ] {
        let spare = field(settings);
        assert_eq!(
            ids(&[("domains/pantry/entities/spare.jadpo", &spare)]),
            [expected],
            "{settings}"
        );
    }
    let text = "entity Spare {\n    id: Uuid\n    label: Text {\n        store: sheets\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n";
    assert!(
        ids(&[("domains/pantry/entities/spare.jadpo", text)])
            .contains(&"type.unknown_setting".to_string())
    );
}

#[test]
fn a_put_needs_its_room_mapped_to_an_unavailable_failure() {
    let unmapped = action(
        "action keep(sheet_text: SheetText) -> File {\n    return attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv)\n}\n",
    );
    assert_eq!(
        ids(&[("domains/pantry/actions/keep.jadpo", &unmapped)]),
        ["failure.rule_unmapped"]
    );
    let invalid = action(
        "action keep(sheet_text: SheetText) fails BadSheet -> File {\n    return attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {\n        invalid: BadSheet\n    }\n}\n",
    );
    assert_eq!(
        ids(&[("domains/pantry/actions/keep.jadpo", &invalid)]),
        ["failure.rule_unmapped"]
    );
    let kind = action(
        "action keep(sheet_text: SheetText) fails BadSheet -> File {\n    return attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {\n        storage_full: BadSheet\n    }\n}\n",
    );
    assert_eq!(
        ids(&[("domains/pantry/actions/keep.jadpo", &kind)]),
        ["failure.mapping_kind"]
    );
}

#[test]
fn only_an_action_calls_a_store_and_only_the_runtime_builds_a_file() {
    let function = "use app { sheets }\nuse pantry/types { SheetText }\n\nfunction keep(sheet_text: SheetText) -> FileContents {\n    return sheets.read(sheets.put(encoding.to_utf8(sheet_text), FileType.csv))\n}\n";
    assert!(
        ids(&[("domains/pantry/libraries/keep.jadpo", function)])
            .contains(&"effect.function_persistence".to_string())
    );
    let built = action(
        "action keep(sheet_text: SheetText) -> File {\n    return File {\n        key: sheet_text\n    }\n}\n",
    );
    assert_eq!(
        ids(&[("domains/pantry/actions/keep.jadpo", &built)]),
        ["type.not_record"]
    );
}

#[test]
fn a_whole_read_of_a_field_over_ten_megabytes_is_refused() {
    let large = "entity BigSheet {\n    id: Uuid\n    sheet: File {\n        store: sheets\n        max_size: 11mb\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n";
    let read = "use app { sheets }\nuse pantry/entities { BigSheet }\n\naction whole(sheet: BigSheet.sheet) -> FileContents {\n    return sheets.read(sheet)\n}\n";
    assert_eq!(
        ids(&[
            ("domains/pantry/entities/big_sheet.jadpo", large),
            ("domains/pantry/actions/whole.jadpo", read),
        ]),
        ["type.file_read_too_large"]
    );
}

#[test]
fn a_file_helpers_rows_are_an_input_a_cell_one_value_and_a_column_read_once() {
    let call = |row_type: &str, file: &str, helper: &str| {
        format!(
            "use pantry/types {{ {row_type} }}\nuse pantry/actions {{ note_listed }}\n\naction read_sheet(sheet: File, whole: FileContents) -> BatchRun {{\n    return {helper}.read({file}, as: {row_type}, each: listed -> note_listed(listed))\n}}\n"
        )
    };
    let listed = |row_type: &str| {
        format!(
            "use pantry/types {{ {row_type} }}\n\naction note_listed(listed: {row_type}) -> Unit {{\n    assert listed == listed\n}}\n"
        )
    };
    let lists = (
        "domains/pantry/types/list_row.jadpo",
        "use pantry/types { ShelfCode }\n\ntype ListRow = Input {\n    codes: List<ShelfCode>\n}\n",
    );
    let twice = (
        "domains/pantry/types/twice_row.jadpo",
        "use pantry/types { ShelfCode }\n\ntype TwiceRow = Input {\n    code: ShelfCode\n    again: ShelfCode {\n        column: \"code\"\n    }\n}\n",
    );
    let object = (
        "domains/pantry/types/shelf_note.jadpo",
        "use pantry/types { ShelfCode }\n\ntype ShelfNote = Object {\n    code: ShelfCode\n}\n",
    );
    for (row_type, extra, file, helper, expected) in [
        (
            "ListRow",
            lists,
            "sheet",
            "csv",
            vec!["type.batch_row_invalid"],
        ),
        (
            "ListRow",
            lists,
            "sheet",
            "tsv",
            vec!["type.batch_row_invalid"],
        ),
        ("ListRow", lists, "sheet", "json_lines", vec![]),
        (
            "TwiceRow",
            twice,
            "sheet",
            "csv",
            vec!["type.batch_row_invalid"],
        ),
        (
            "ShelfNote",
            object,
            "sheet",
            "json_lines",
            vec!["type.batch_row_invalid"],
        ),
        (
            "ListRow",
            lists,
            "whole",
            "json_lines",
            vec!["type.mismatch"],
        ),
    ] {
        let read = call(row_type, file, helper);
        let note = listed(row_type);
        let found = ids(&[
            extra,
            ("domains/pantry/actions/note_listed.jadpo", &note),
            ("domains/pantry/actions/read_sheet.jadpo", &read),
        ]);
        assert_eq!(found, expected, "{row_type} {file} {helper}");
    }
    // Only an action starts a run; the writers are `exports.rs`'s.
    let function = "use pantry/types { ShelfRow }\n\nfunction read_sheet(sheet: File) -> BatchRun {\n    return csv.read(sheet, as: ShelfRow, each: listed -> note_shelf(listed))\n}\n";
    assert!(
        ids(&[("domains/pantry/libraries/read_sheet.jadpo", function)])
            .contains(&"type.batch_context".to_string())
    );
}

/// `json.read` given a stored `File` and `each:` is its file form (chunk
/// 6.4b3): a run over the list `items_at:` names, a text literal of
/// member names, each item an `Input` that may hold lists and objects;
/// given text it is still the in-memory read. Every reader takes
/// `max_unpacked_size:`, a size literal, and only `json.read` takes
/// `items_at:`.
#[test]
fn json_reads_a_files_list_where_items_at_says_and_any_reader_unpacks() {
    let lists = (
        "domains/pantry/types/list_row.jadpo",
        "use pantry/types { ShelfCode }\n\ntype ListRow = Input {\n    codes: List<ShelfCode>\n}\n",
    );
    let note = (
        "domains/pantry/actions/note_listed.jadpo",
        "use pantry/types { ListRow }\n\naction note_listed(listed: ListRow) -> Unit {\n    assert listed == listed\n}\n",
    );
    let read = |args: &str| {
        format!(
            "use pantry/types {{ ListRow, SheetText }}\nuse pantry/actions {{ note_listed }}\n\naction read_sheet(sheet: File, label: SheetText) -> BatchRun {{\n    return {args}\n}}\n"
        )
    };
    for (call, expected) in [
        (
            "json.read(sheet, items_at: \"data.items\", as: ListRow, each: listed -> note_listed(listed))",
            vec![],
        ),
        // The document itself is the list.
        (
            "json.read(sheet, as: ListRow, each: listed -> note_listed(listed))",
            vec![],
        ),
        (
            "json.read(sheet, items_at: \"items\", as: ListRow, each: listed -> note_listed(listed), max_unpacked_size: 200mb)",
            vec![],
        ),
        (
            "json_lines.read(sheet, as: ListRow, each: listed -> note_listed(listed), max_unpacked_size: 1gb)",
            vec![],
        ),
        // `items_at:` is written out, member names between dots.
        (
            "json.read(sheet, items_at: label, as: ListRow, each: listed -> note_listed(listed))",
            vec!["type.batch_row_invalid"],
        ),
        (
            "json.read(sheet, items_at: \"data..items\", as: ListRow, each: listed -> note_listed(listed))",
            vec!["type.batch_row_invalid"],
        ),
        (
            "json.read(sheet, items_at: \"\", as: ListRow, each: listed -> note_listed(listed))",
            vec!["type.batch_row_invalid"],
        ),
        // A size is a whole number and its unit.
        (
            "json.read(sheet, as: ListRow, each: listed -> note_listed(listed), max_unpacked_size: 200)",
            vec!["type.batch_row_invalid"],
        ),
        (
            "json.read(sheet, as: ListRow, each: listed -> note_listed(listed), max_unpacked_size: 0mb)",
            vec!["type.batch_row_invalid"],
        ),
        // Only `json.read` takes `items_at:`.
        (
            "json_lines.read(sheet, items_at: \"items\", as: ListRow, each: listed -> note_listed(listed))",
            vec!["type.named_argument_unknown"],
        ),
    ] {
        let source = read(call);
        let found = ids(&[
            lists,
            note,
            ("domains/pantry/actions/read_sheet.jadpo", &source),
        ]);
        assert_eq!(found, expected, "{call}");
    }
    // A CSV row still holds one value a cell, unpacked or not.
    let csv = "use pantry/types { ListRow }\nuse pantry/actions { note_listed }\n\naction read_sheet(sheet: File) -> BatchRun {\n    return csv.read(sheet, as: ListRow, each: listed -> note_listed(listed), max_unpacked_size: 5mb)\n}\n";
    assert_eq!(
        ids(&[
            lists,
            note,
            ("domains/pantry/actions/read_sheet.jadpo", csv)
        ]),
        ["type.batch_row_invalid"]
    );
    // Given text, `json.read` reads one document in memory, as before.
    let text = "use pantry/types { ListRow, SheetText }\n\naction listed(sheet_text: SheetText) fails BadSheet -> ListRow {\n    return attempt json.read(sheet_text, as: ListRow) {\n        malformed: BadSheet\n        mismatch: BadSheet\n    }\n}\n";
    let failures = "use pantry/failures { BadSheet }\n";
    let function = format!("{failures}{text}");
    assert_eq!(
        ids(&[lists, ("domains/pantry/actions/listed.jadpo", &function)]),
        Vec::<String>::new()
    );
}
