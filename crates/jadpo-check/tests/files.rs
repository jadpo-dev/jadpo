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

/// A disk store's settings (area 16, "The disk driver"; chunk B2): `path:`
/// an absolute folder written plainly, `free_space_floor:` a percentage
/// below 100%, and no other key; a floor the plan carries in millionths
/// of the disk, 5% when left out.
#[test]
fn a_disk_stores_settings_are_held_to_what_they_say() {
    let app = |settings: &str| {
        format!(
            "application {{\n    connections: {{\n        sheets: {{\n            driver: Driver.disk\n{settings}        }}\n    }}\n}}\n"
        )
    };
    let rules = |settings: &str| {
        let app = app(settings);
        let mut files: Vec<(&str, &str)> = vec![("app.jadpo", &app)];
        files.extend_from_slice(BASE);
        support::rules(Layout::Project, &files)
            .into_iter()
            .filter(|rule| {
                rule != "import.unused"
                    && rule != "layout.unused_declaration"
                    && !rule.starts_with("migration.")
            })
            .collect::<Vec<_>>()
    };
    for settings in [
        "            path: \"/var/lib/pantry/sheets\"\n",
        "            path: \"/var/lib/pantry/sheets\"\n            free_space_floor: 5%\n",
        "            path: \"/var/lib/pantry/sheets\"\n            free_space_floor: 0%\n",
        "            path: \"/var/lib/pantry/sheets\"\n            free_space_floor: 2.5%\n",
        // No `path:` checks: a deployable lists the store as unbuilt.
        "",
    ] {
        assert_eq!(rules(settings), Vec::<String>::new(), "{settings}");
    }
    for (settings, expected) in [
        (
            "            path: \"uploads\"\n",
            "connection.disk_path_invalid",
        ),
        (
            "            path: \"./uploads\"\n",
            "connection.disk_path_invalid",
        ),
        (
            "            path: \"/var/lib/pantry/../etc\"\n",
            "connection.disk_path_invalid",
        ),
        ("            path: \"\"\n", "connection.disk_path_invalid"),
        ("            path: 5\n", "connection.value_invalid"),
        (
            "            path: \"/var/lib/pantry/sheets\"\n            free_space_floor: 100%\n",
            "connection.value_invalid",
        ),
        (
            "            path: \"/var/lib/pantry/sheets\"\n            free_space_floor: -1%\n",
            "connection.value_invalid",
        ),
        (
            "            path: \"/var/lib/pantry/sheets\"\n            free_space_floor: 5mb\n",
            "connection.value_invalid",
        ),
        (
            "            path: \"/var/lib/pantry/sheets\"\n            free_space: 5%\n",
            "connection.key_invalid",
        ),
    ] {
        assert_eq!(rules(settings), vec![expected.to_string()], "{settings}");
    }
}

/// Fields naming one store of files may be in entities of two databases
/// (area 16, "Deleting and erasing"; chunk B14): each counts its own
/// records' holders, and the runtime deletes a file's bytes only once
/// neither holds it, so neither field is refused.
#[test]
fn a_store_of_files_held_from_two_databases_checks() {
    let app = "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        archive: {\n            driver: Driver.postgres\n        }\n        sheets: {\n            driver: Driver.disk\n            path: \"/var/lib/pantry/sheets\"\n        }\n        notes: {\n            driver: Driver.disk\n            path: \"/var/lib/pantry/notes\"\n        }\n    }\n}\n";
    let old = |store: &str| {
        format!(
            "entity OldSheet {{\n    id: Uuid\n    sheet: File? {{\n        store: {store}\n        max_size: 5mb\n    }}\n\n    store: archive\n\n    policy: {{\n        create: [Access.public]\n        read: [Access.public]\n    }}\n}}\n"
        )
    };
    let rules = |store: &str| {
        let old = old(store);
        let mut files: Vec<(&str, &str)> = vec![("app.jadpo", app)];
        files.extend_from_slice(BASE);
        files.push(("domains/pantry/entities/old_sheet.jadpo", &old));
        let found = support::problems(Layout::Project, &files);
        found
            .into_iter()
            .filter(|problem| {
                !problem.contains("import.unused")
                    && !problem.contains("layout.unused_declaration")
                    && !problem.contains("migration.")
            })
            .collect::<Vec<_>>()
    };
    // Its own store of files, counted in its own database: nothing.
    assert_eq!(rules("notes"), Vec::<String>::new());
    // One store of files named from both databases: nothing either.
    assert_eq!(rules("sheets"), Vec::<String>::new());
}

/// What the base project reports with `Sheet`'s declaration and a route
/// that sends `output`, with `body` as what its action does after reading
/// the record, but for what [`ids`] leaves out.
fn answering(types: &str, output: &str, body: &str) -> Vec<String> {
    let action = format!(
        "use pantry/entities {{ ShelfSheet, ShelfSheetNotFound }}\nuse pantry/types {{ Sheet }}\n\naction answer(id: ShelfSheet.id) fails ShelfSheetNotFound -> {output} {{\n    var kept: ShelfSheet = attempt ShelfSheet.get(id)\n    {body}\n}}\n"
    );
    let routes = format!(
        "use pantry/actions {{ answer }}\nuse pantry/entities {{ ShelfSheet }}\nuse pantry/types {{ Sheet }}\n\nroutes Sheets {{\n    prefix: /sheets\n\n    GET /{{id: ShelfSheet.id}} {{\n        authentication: RouteAuthentication.none\n        output: {output}\n        run: answer(id)\n    }}\n}}\n"
    );
    let types = format!("use pantry/entities {{ ShelfSheet }}\n\n{types}");
    ids(&[
        ("domains/pantry/types/sheet.jadpo", &types),
        ("domains/pantry/actions/answer.jadpo", &action),
        ("domains/pantry/routes/sheets.jadpo", &routes),
    ])
    .into_iter()
    .filter(|rule| !rule.starts_with("record."))
    .collect()
}

/// An answer sends a `File` as a signed link (area 16, "Downloading"), so a
/// `File` reaching a route's output, directly or through what it holds,
/// is not refused.
#[test]
fn a_file_in_a_routes_output_is_sent_as_a_link() {
    const REFUSED: &str = "toolchain.not_built_yet";
    let kept = "return Sheet {\n        sheet: kept.sheet\n    }";
    for (what, types, output, body) in [
        (
            "a field",
            "type Sheet = Output {\n    sheet: File\n}\n",
            "Sheet",
            kept,
        ),
        (
            "a field named for the entity's",
            "type Sheet = Output {\n    sheet: ShelfSheet.sheet\n}\n",
            "Sheet",
            kept,
        ),
        (
            "a pick",
            "type Sheet = Output {\n    pick: [ShelfSheet.sheet]\n}\n",
            "Sheet",
            kept,
        ),
        (
            "a list of an output that holds one",
            "type Sheet = Output {\n    sheet: File\n}\n",
            "List<Sheet>",
            "return [Sheet {\n        sheet: kept.sheet\n    }]",
        ),
        (
            "a list in a field",
            "type Sheet = Output {\n    sheets: List<File>\n}\n",
            "Sheet",
            "return Sheet {\n        sheets: [kept.sheet]\n    }",
        ),
        (
            "an optional field",
            "type Sheet = Output {\n    sheet: File?\n}\n",
            "Sheet",
            kept,
        ),
        (
            "an object it holds",
            "type Inner = Object {\n    file: File\n}\n\ntype Sheet = Output {\n    inner: Inner\n}\n",
            "Sheet",
            "return Sheet {\n        inner: Inner {\n            file: kept.sheet\n        }\n    }",
        ),
        (
            "a map layer's properties",
            "type Sheet = Object {\n    sheet: File\n}\n",
            "FeatureCollection<Point, Sheet>",
            kept,
        ),
        (
            "an export's summary, which holds the file it wrote",
            "type Sheet = Output {\n    summary: ExportSummary\n}\n",
            "Sheet",
            kept,
        ),
    ] {
        let found = answering(types, output, body);
        assert_eq!(
            found.iter().filter(|rule| *rule == REFUSED).count(),
            0,
            "{what}: {found:?}"
        );
    }
    // An output that leaves the file out is an answer like any other.
    assert_eq!(
        answering(
            "type Sheet = Output {\n    pick: [ShelfSheet.id]\n}\n",
            "Sheet",
            "return Sheet {\n        id: kept.id\n    }",
        ),
        Vec::<String>::new()
    );
}

/// A failure's `client:` fields are sent in its answer, so a `File` there
/// is a signed link as one in an output is; its `internal:` ones stay on
/// the server.
#[test]
fn a_file_in_a_failures_client_fields_is_sent_as_a_link() {
    let failing = |section: &str| {
        let failure = format!(
            "type SheetRefused = Failure {{\n    kind: FailureKind.conflict\n    {section}: {{\n        sheet: File\n    }}\n}}\n"
        );
        ids(&[("domains/pantry/failures/sheet_refused.jadpo", &failure)])
    };
    assert_eq!(failing("client"), Vec::<String>::new());
    assert_eq!(failing("internal"), Vec::<String>::new());
}

/// The runtime answers a store of files' links before any route, so a
/// route at one would never run (area 16, "Downloading"); the same path
/// is an ordinary route when nothing keeps files under that name.
#[test]
fn a_route_at_a_stores_download_links_is_refused() {
    let route = |path: &str| {
        let routes = format!(
            "use pantry/actions {{ whoever }}\nuse pantry/entities {{ ShelfSheet }}\n\nroutes Files {{\n    prefix: /files\n\n    GET {path} {{\n        authentication: RouteAuthentication.none\n        status: SuccessStatus.no_content\n        run: whoever(link)\n    }}\n}}\n"
        );
        ids(&[
            (
                "domains/pantry/actions/whoever.jadpo",
                "use pantry/entities { ShelfSheet }\n\naction whoever(id: ShelfSheet.id) -> Unit {\n    assert id == id\n}\n",
            ),
            ("domains/pantry/routes/files.jadpo", &routes),
        ])
        .into_iter()
        .filter(|rule| rule.starts_with("route."))
        .collect::<Vec<_>>()
    };
    assert_eq!(
        route("/sheets/{link: ShelfSheet.id}"),
        ["route.path_overlap"]
    );
    assert_eq!(route("/scans/{link: ShelfSheet.id}"), Vec::<String>::new());
}

/// Only the runtime builds a `File`, and one is the store and key its
/// bytes are under, so no request carries one (area 16, "What a `File`
/// holds"): it would name any file, another person's included.
#[test]
fn a_file_in_an_input_is_refused() {
    for (what, types) in [
        ("a field", "type Sent = Input {\n    sheet: File\n}\n"),
        ("a list", "type Sent = Input {\n    sheets: List<File>\n}\n"),
        (
            "an object it holds",
            "type Inner = Object {\n    file: File\n}\n\ntype Sent = Input {\n    inner: Inner\n}\n",
        ),
    ] {
        let found = ids(&[("domains/pantry/types/sent.jadpo", types)]);
        assert_eq!(found, ["type.batch_input"], "{what}");
    }
}

const ACME: &str = "shared/providers/acme/v1";

/// The base project connected to a provider package with webhooks: `keys`
/// are the `input:`, `output:` and other keys of its `uploads.create`,
/// `event` what its `Asked` event holds, and `types` its `types/`, each a
/// name and what its `Object` holds. Reports each problem as `rule: message`,
/// but for what [`ids`] leaves out.
fn through_acme(
    keys: &str,
    event: &str,
    types: &[(&str, &str)],
    more: &[(&str, &str)],
) -> Vec<String> {
    let app = "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        sheets: {\n            driver: Driver.disk\n            path: \"/var/lib/pantry/sheets\"\n        }\n        acme: {\n            package: Acme.v1\n            domains: [pantry]\n            credential: config.acme_key\n            host: config.acme_host\n            webhooks: {\n                secret: config.acme_secret\n            }\n        }\n    }\n    events: {\n        store: main\n    }\n}\n";
    let config = "config {\n    acme_host: Url\n    acme_key: Text {\n        secret: true\n    }\n    acme_secret: Text {\n        secret: true\n    }\n}\n";
    let resource = format!(
        "public resource uploads {{\n    create: {{\n        method: HttpMethod.post\n        path: \"/v1/uploads\"\n{keys}        changes: false\n        source: PackageSource.written\n    }}\n}}\n"
    );
    let asked = format!(
        "/// Acme tells the app something.\npublic type Asked = Event {{\n    wire_name: \"asked\"\n    occurred_at: Instant\n{event}}}\n"
    );
    let mut owned: Vec<(String, String)> = vec![
        ("app.jadpo".into(), app.into()),
        ("config.jadpo".into(), config.into()),
        (
            format!("{ACME}/package.jadpo"),
            "/// Acme.\npackage Acme.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n".into(),
        ),
        (
            format!("{ACME}/provider.jadpo"),
            "/// Acme's API.\nprovider {\n    protocol: Protocol.json_http\n    credential: CredentialScheme.bearer\n    webhooks: {\n        check: SignatureScheme.stripe\n        event_id: \"id\"\n        event_type: \"type\"\n        occurred_at: \"created\"\n    }\n}\n".into(),
        ),
        (format!("{ACME}/resources/uploads.jadpo"), resource),
        (format!("{ACME}/events/asked.jadpo"), asked),
    ];
    for (name, holds) in types {
        owned.push((
            format!("{ACME}/types/{}.jadpo", name.to_lowercase()),
            format!("/// {name}.\npublic type {name} = Object {{\n{holds}}}\n"),
        ));
    }
    let mut files: Vec<(&str, &str)> = BASE.to_vec();
    files.extend(
        owned
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str())),
    );
    files.extend_from_slice(more);
    support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| {
            !["import.unused", "layout.unused_declaration"]
                .iter()
                .any(|rule| problem.contains(&format!(" {rule}:")))
                && !problem.contains(" migration.")
                && !problem.contains(" record.")
        })
        .map(|problem| {
            problem
                .split_once(' ')
                .map_or(problem.clone(), |(_, rest)| rest.to_string())
        })
        .collect()
}

/// Each way a type's field may hold a `File`, as the field is declared.
/// `Inner` is an `Object` holding one.
const HOLDING: [(&str, &str); 6] = [
    ("a field", "    sheet: File\n"),
    ("an optional field", "    sheet: File?\n"),
    ("a list", "    sheets: List<File>\n"),
    ("a map's values", "    sheets: Map<Text, File>\n"),
    ("an object it holds", "    inner: List<Inner>\n"),
    ("an export's summary", "    summary: ExportSummary\n"),
];

const INNER: (&str, &str) = ("Inner", "    sheet: File\n");
const PLAIN: (&str, &str) = ("Plain", "    name: Text\n");

/// A `File` sent to a provider goes as its bytes (area 11, "Types";
/// chunk B14): an operation's `input:` or a webhook's `reply:` holding one,
/// however deep, checks like any other, and a call sending one too.
#[test]
fn a_file_sent_to_a_provider_checks() {
    for (what, holds) in HOLDING {
        let found = through_acme(
            "        input: Sent\n        output: Plain\n",
            "",
            &[("Sent", holds), INNER, PLAIN],
            &[],
        );
        assert_eq!(found, Vec::<String>::new(), "an operation's input, {what}");
        let found = through_acme(
            "        input: Plain\n        output: Plain\n",
            "    reply: Sent\n",
            &[("Sent", holds), INNER, PLAIN],
            &[],
        );
        // A connection that doesn't answer the event is reported beside it.
        assert!(
            !found
                .iter()
                .any(|p| p.starts_with("toolchain.not_built_yet:")
                    || p.starts_with("type.batch_input:")),
            "a webhook's reply, {what}: {found:?}"
        );
    }
    let action = "use app { acme, sheets }\nuse pantry/failures { NoRoom }\nuse shared/providers/acme\n\naction send() fails NoRoom -> Unit {\n    var sheet: File = attempt sheets.put(encoding.to_utf8(\"a,b\"), FileType.csv) {\n        storage_full: NoRoom\n    }\n    var plain: Acme.Plain = acme.uploads.create(\n        Acme.Sent {\n            sheet: sheet\n        }\n    )\n    assert plain.name == \"sent\"\n}\n";
    let found = through_acme(
        "        input: Sent\n        output: Plain\n",
        "",
        &[("Sent", "    sheet: File\n"), PLAIN],
        &[("domains/pantry/actions/send.jadpo", action)],
    );
    assert_eq!(found, Vec::<String>::new());
}

/// Only the runtime builds a `File`, so nothing from outside carries one:
/// an operation's `output:` or `wire_output:` and a webhook event's fields,
/// holding one however deep, are `type.batch_input`, since the provider
/// could name any file's store and key (area 16, "What a `File` holds").
#[test]
fn a_file_taken_from_a_provider_is_refused() {
    const REFUSED: &str = "type.batch_input:";
    for (what, holds) in HOLDING {
        let found = through_acme(
            "        input: Plain\n        output: Answer\n",
            "",
            &[("Answer", holds), INNER, PLAIN],
            &[],
        );
        assert_eq!(found.len(), 1, "an operation's output, {what}: {found:?}");
        assert!(
            found[0].starts_with(REFUSED),
            "an output, {what}: {found:?}"
        );
        let found = through_acme(
            "        input: Plain\n        output: Plain\n",
            holds,
            &[INNER, PLAIN],
            &[],
        );
        assert_eq!(found.len(), 1, "a webhook's event, {what}: {found:?}");
        assert!(found[0].starts_with(REFUSED), "an event, {what}: {found:?}");
    }
    // An output that is a list of what holds one, or a `File` itself.
    for output in ["List<Inner>", "File"] {
        let found = through_acme(
            &format!("        input: Plain\n        output: {output}\n"),
            "",
            &[INNER, PLAIN],
            &[],
        );
        assert_eq!(found.len(), 1, "`output: {output}`: {found:?}");
        assert!(
            found[0].starts_with(REFUSED),
            "`output: {output}`: {found:?}"
        );
    }
    // The answer read whole, before the package's own function sees it.
    let decode = "public library wire {\n    /// Reads the answer.\n    function read(sent: Inner) -> Plain {\n        return Plain {\n            name: \"read\"\n        }\n    }\n}\n";
    let found = through_acme(
        "        input: Plain\n        output: Plain\n        wire_output: Inner\n        decode: wire.read\n",
        "",
        &[INNER, PLAIN],
        &[("shared/providers/acme/v1/libraries/wire.jadpo", decode)],
    );
    assert_eq!(found.len(), 1, "`wire_output:`: {found:?}");
    assert!(found[0].starts_with(REFUSED), "`wire_output:`: {found:?}");
    // What it says names what comes in and where the file is.
    assert_eq!(
        through_acme(
            "        input: Plain\n        output: Plain\n",
            "    sheet: File\n",
            &[PLAIN],
            &[],
        ),
        [
            "type.batch_input: `Asked` holds a `File`, in `Acme.Asked.sheet`, which nothing from outside carries"
        ]
    );
}

/// A program reads none of a `File`'s parts, not one a `File` has and not
/// one it hasn't (area 16, "What a `File` holds"), however the value got
/// there.
#[test]
fn reading_a_part_of_a_file_is_refused() {
    const REFUSED: &str = "type.field_on_non_record";
    for part in ["store", "key", "name", "size", "type", "nope"] {
        let local = action(&format!(
            "action part(id: ShelfSheet.id) fails ShelfSheetNotFound -> Unit {{\n    var kept: ShelfSheet = attempt ShelfSheet.get(id)\n    assert kept.sheet.{part} == kept.sheet.{part}\n}}\n"
        ));
        let found = ids(&[("domains/pantry/actions/part.jadpo", &local)]);
        assert!(
            found.iter().any(|rule| rule == REFUSED),
            "`kept.sheet.{part}`: {found:?}"
        );
        assert!(
            found
                .iter()
                .all(|rule| rule == REFUSED || rule == "semantic.unused_variable"),
            "`kept.sheet.{part}`: {found:?}"
        );
    }
    for (what, body) in [
        (
            "a parameter",
            "action part(sheet: ShelfSheet.sheet) -> Unit {\n    assert sheet.key == sheet.key\n}\n",
        ),
        (
            "a `File` parameter",
            "action part(sheet: File) -> Unit {\n    assert sheet.key == sheet.key\n}\n",
        ),
        (
            "an optional `File`",
            "action part(sheet: File?) -> Unit {\n    assert sheet.key == sheet.key\n}\n",
        ),
        (
            "an item of a list",
            "action part(sheets: List<File>) -> Unit {\n    for sheet in sheets {\n        assert sheet.key == sheet.key\n    }\n}\n",
        ),
        (
            "a lambda's parameter",
            "action part(sheets: List<File>) -> Unit {\n    assert list.count(list.filter(sheets, sheet -> sheet.size > 0)) == 0\n}\n",
        ),
        (
            "an export's file",
            "action part(summary: ExportSummary) -> Unit {\n    assert summary.file.size > 0\n}\n",
        ),
    ] {
        let found = ids(&[("domains/pantry/actions/part.jadpo", &action(body))]);
        assert!(
            found.iter().any(|rule| rule == REFUSED),
            "{what}: {found:?}"
        );
    }
    // Its bytes have no parts either.
    let found = ids(&[(
        "domains/pantry/actions/part.jadpo",
        &action(
            "action part(sheet: File) -> Unit {\n    var whole: FileContents = sheets.read(sheet)\n    assert whole.size > 0\n}\n",
        ),
    )]);
    assert!(found.iter().any(|rule| rule == REFUSED), "{found:?}");
    // What it says names the value and what a program does with one.
    let problems = support::problems(
        Layout::Project,
        &[
            APP,
            BASE[3],
            (
                "domains/pantry/actions/part.jadpo",
                "action part(sheet: File) -> Unit {\n    assert sheet.key == sheet.key\n}\n",
            ),
        ],
    );
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("type.field_on_non_record: `File` has no fields")),
        "{problems:?}"
    );
}

const MAIL_APP: (&str, &str) = (
    "app.jadpo",
    "application {\n    display_name: \"Pantry\"\n    connections: {\n        sheets: {\n            driver: Driver.disk\n            path: \"/var/lib/pantry/sheets\"\n        }\n        mail: {\n            driver: Driver.smtp\n            host: \"smtp.pantry.example\"\n        }\n    }\n    email: {\n        connection: mail\n        from: \"hello@pantry.example\"\n    }\n    events: {\n        store: main\n    }\n}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb]\n}\n",
);

/// What a project with an email holding `fields`, and a template writing
/// `template`, reports, but for what these projects don't keep.
fn emailing(fields: &str, template: &str, more: &[(&str, &str)]) -> Vec<String> {
    let using = if fields.contains("HeldSheet") {
        "use pantry/types { HeldSheet }\n\n"
    } else {
        ""
    };
    let email =
        format!("{using}/// A sheet to read.\ntype SheetEmail = EmailMessage {{\n{fields}}}\n");
    let mut files: Vec<(&str, &str)> = vec![
        MAIL_APP,
        (
            "domains/pantry/translations/en_gb.jadpo",
            "translations {\n    sheet_email: \"Your sheet\"\n}\n",
        ),
        ("domains/pantry/emails/sheet_email.jadpo", &email),
        ("domains/pantry/emails/sheet_email.txt", template),
    ];
    files.extend_from_slice(more);
    support::rules(Layout::Project, &files)
        .into_iter()
        .filter(|rule| {
            rule != "import.unused"
                && rule != "layout.unused_declaration"
                && !rule.starts_with("migration.")
        })
        .collect()
}

/// A template reads none of a `File`'s parts: it was written out blank.
#[test]
fn a_template_reading_a_part_of_a_file_is_refused() {
    for read in [
        "{{ sheet.key }}\n",
        "{{ sheet.nope }}\n",
        "{{ if (sheet.size > 0) }}\nA sheet.\n{{ end }}\n",
    ] {
        let found = emailing(
            "    sheet: File {\n        store: sheets\n        max_size: 5mb\n    }\n",
            read,
            &[],
        );
        assert_eq!(found, ["type.field_on_non_record"], "{read}");
    }
    // An email with its attachment, whose template doesn't read it.
    assert_eq!(
        emailing(
            "    sheet: File {\n        store: sheets\n        max_size: 5mb\n    }\n",
            "A sheet.\n",
            &[],
        ),
        Vec::<String>::new()
    );
}

/// An email's attachments are its fields of files (area 16, "Attachments";
/// chunk B14): a `File`, a `FileContents`, or a list, set or map of them,
/// each sent. A partial sends nothing, so a file on one is
/// `toolchain.not_built_yet`; an object is read for its other fields.
#[test]
fn an_email_attaches_each_file_its_fields_hold() {
    const INNER: (&str, &str) = (
        "domains/pantry/types/held_sheet.jadpo",
        "/// A sheet.\ntype HeldSheet = Object {\n    title: Text\n    sheet: File\n}\n",
    );
    for (what, field) in [
        (
            "a file",
            "    sheet: File {\n        store: sheets\n        max_size: 5mb\n    }\n",
        ),
        (
            "an optional file",
            "    sheet: File? {\n        store: sheets\n        max_size: 5mb\n    }\n",
        ),
        ("a list", "    sheets: List<File>\n"),
        ("an optional list", "    sheets: List<File>?\n"),
        ("a set", "    sheets: Set<File>\n"),
        ("a map", "    sheets: Map<Text, File>\n"),
        ("a list of lists", "    sheets: List<List<File>>\n"),
        ("its bytes", "    whole: FileContents\n"),
        ("a list of its bytes", "    wholes: List<FileContents>\n"),
    ] {
        let found = emailing(field, "A sheet.\n", &[INNER]);
        assert_eq!(found, Vec::<String>::new(), "{what}");
    }
    // A partial sends nothing, so no file on one is an attachment.
    for field in [
        "    sheet: File\n",
        "    sheets: List<File>\n",
        "    whole: FileContents\n",
    ] {
        let partial =
            format!("/// How an email ends.\ntype SignOff = EmailPartial {{\n{field}}}\n");
        let found = emailing(
            "    title: Text\n",
            "{{ title }}\n",
            &[
                ("domains/pantry/emails/sign_off.jadpo", &partial),
                ("domains/pantry/emails/sign_off.txt", "Thanks.\n"),
            ],
        );
        assert_eq!(found, ["toolchain.not_built_yet"], "a partial, {field}");
    }
    // An object is read for its other fields, and a summary for its counts.
    assert_eq!(
        emailing("    held: HeldSheet\n", "{{ held.title }}\n", &[INNER]),
        Vec::<String>::new()
    );
    assert_eq!(
        emailing("    summary: ExportSummary\n", "{{ summary.items }}\n", &[]),
        Vec::<String>::new()
    );
}

/// `store.read`'s bytes are sent to a provider, as Base64 in its JSON: the
/// one way out area 16 names. Nothing decodes one, so it comes in from
/// nowhere; a route sending one is `toolchain.not_built_yet`, as a `File` is.
#[test]
fn a_files_bytes_go_to_a_provider_and_nowhere_else() {
    const SENT: (&str, &str) = ("Sent", "    whole: FileContents\n");
    assert_eq!(
        through_acme(
            "        input: Sent\n        output: Plain\n",
            "",
            &[SENT, PLAIN],
            &[],
        ),
        Vec::<String>::new(),
        "an operation's input"
    );
    assert_eq!(
        through_acme(
            "        input: Plain\n        output: Sent\n",
            "",
            &[SENT, PLAIN],
            &[],
        ),
        [
            "type.batch_input: `uploads.create`'s `output:` holds a `FileContents`, in `Acme.Sent.whole`, which nothing from outside carries"
        ]
    );
    assert_eq!(
        through_acme(
            "        input: Plain\n        output: Plain\n",
            "    whole: FileContents\n",
            &[PLAIN],
            &[],
        ),
        [
            "type.batch_input: `Asked` holds a `FileContents`, in `Acme.Asked.whole`, which nothing from outside carries"
        ]
    );
    // The answer read whole, before the package's own function sees it.
    let decode = "public library wire {\n    /// Reads the answer.\n    function read(sent: Sent) -> Plain {\n        return Plain {\n            name: \"read\"\n        }\n    }\n}\n";
    assert_eq!(
        through_acme(
            "        input: Plain\n        output: Plain\n        wire_output: Sent\n        decode: wire.read\n",
            "",
            &[SENT, PLAIN],
            &[("shared/providers/acme/v1/libraries/wire.jadpo", decode)],
        ),
        [
            "type.batch_input: `uploads.create`'s `wire_output:` holds a `FileContents`, in `Acme.Sent.whole`, which nothing from outside carries"
        ]
    );
    for (what, field) in [
        ("a field", "    whole: FileContents\n"),
        ("an optional field", "    whole: FileContents?\n"),
        ("a list", "    wholes: List<FileContents>\n"),
    ] {
        let found = ids(&[(
            "domains/pantry/types/sent.jadpo",
            &format!("type Sent = Input {{\n{field}}}\n"),
        )]);
        assert_eq!(found, ["type.batch_input"], "an input, {what}");
    }
    let found = answering(
        "type Sheet = Output {\n    whole: FileContents\n}\n",
        "Sheet",
        "return Sheet {\n        whole: sheets.read(kept.sheet)\n    }",
    );
    assert!(
        found.iter().any(|rule| rule == "toolchain.not_built_yet"),
        "{found:?}"
    );
    let found = ids(&[(
        "domains/pantry/failures/bad_bytes.jadpo",
        "type BadBytes = Failure {\n    kind: FailureKind.invalid_value\n    client: {\n        whole: FileContents\n    }\n}\n",
    )]);
    assert!(
        found.iter().any(|rule| rule == "toolchain.not_built_yet"),
        "{found:?}"
    );
}

/// A `File` field gives its entity `<field>_upload()` (area 16,
/// "Uploading"), which takes nothing and gives an `UploadSlot`; a name
/// that is no `File` field of the entity is an unknown operation.
#[test]
fn a_file_fields_slot_is_generated() {
    let asked = |body: &str| {
        let source = action(&format!("action asked() -> UploadSlot {{\n{body}\n}}\n"));
        ids(&[("domains/pantry/actions/asked.jadpo", &source)])
    };
    assert_eq!(
        asked("    return ShelfSheet.sheet_upload()"),
        Vec::<String>::new()
    );
    assert_eq!(
        asked("    return ShelfSheet.label_upload()"),
        ["semantic.unknown_callee"]
    );
    assert_eq!(
        asked("    return ShelfSheet.sheet_upload(1)"),
        ["type.argument_count"]
    );
}

/// A slot is an action's: nothing else may ask for one.
#[test]
fn a_function_cant_ask_for_a_slot() {
    let source =
        action("function asked() -> UploadSlot {\n    return ShelfSheet.sheet_upload()\n}\n");
    let found = ids(&[("domains/pantry/functions/asked.jadpo", &source)]);
    assert!(
        found
            .iter()
            .any(|rule| rule == "effect.function_persistence"),
        "{found:?}"
    );
}

/// A field over 100 MB gets a slot in parts, the same `UploadSlot`.
#[test]
fn a_field_over_a_hundred_mb_gets_a_slot_in_parts() {
    let entity = "entity BigSheet {\n    id: Uuid\n    sheet: File {\n        store: sheets\n        max_size: 200mb\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n";
    let source = "use pantry/entities { BigSheet }\n\naction asked() -> UploadSlot {\n    return BigSheet.sheet_upload()\n}\n";
    assert_eq!(
        ids(&[
            ("domains/pantry/entities/big_sheet.jadpo", entity),
            ("domains/pantry/actions/asked.jadpo", source),
        ]),
        Vec::<String>::new()
    );
}

/// A write takes a `FileUpload` for a `File` field, in `set:` and `patch:`
/// alike; nothing else turns a token into a stored file.
#[test]
fn a_write_attaches_an_upload_and_nothing_else_does() {
    const UPLOADED: &str = "domains/pantry/types/sheet_upload.jadpo";
    let upload = "type SheetUpload = Input {\n    sheet: FileUpload\n}\n";
    let attaching = action(
        "action attach(input: SheetUpload) -> ShelfSheet {\n    return ShelfSheet.create {\n        set: {\n            sheet: input.sheet\n        }\n    }\n}\n",
    )
    .replace("use pantry/types { SheetText, ShelfRow }", "use pantry/types { SheetText, ShelfRow, SheetUpload }");
    assert_eq!(
        ids(&[
            (UPLOADED, upload),
            ("domains/pantry/actions/attach.jadpo", &attaching),
        ]),
        Vec::<String>::new()
    );
    // Reading the bytes of a token isn't attaching one.
    let reading = action(
        "action read_it(input: SheetUpload) -> FileContents {\n    return sheets.read(input.sheet)\n}\n",
    )
    .replace("use pantry/types { SheetText, ShelfRow }", "use pantry/types { SheetText, ShelfRow, SheetUpload }");
    assert_eq!(
        ids(&[
            (UPLOADED, upload),
            ("domains/pantry/actions/read_it.jadpo", &reading),
        ]),
        ["type.mismatch"]
    );
}

/// The store is asked before a write that attaches, so the write is held
/// to what any outside call is: it comes before the action's first write
/// (area 6, "Several outside calls").
#[test]
fn an_upload_is_attached_before_the_actions_first_write() {
    let upload = "type SheetUpload = Input {\n    sheet: FileUpload\n}\n";
    let source = action(
        "action attach(input: SheetUpload, other: ShelfSheet) -> ShelfSheet {\n    var first: ShelfSheet = ShelfSheet.create {\n        set: {\n            sheet: other.sheet\n        }\n    }\n    var second: ShelfSheet = ShelfSheet.create {\n        set: {\n            sheet: input.sheet\n        }\n    }\n    assert first.id != second.id\n    return second\n}\n",
    )
    .replace("use pantry/types { SheetText, ShelfRow }", "use pantry/types { SheetText, ShelfRow, SheetUpload }");
    let found = ids(&[
        ("domains/pantry/types/sheet_upload.jadpo", upload),
        ("domains/pantry/actions/attach.jadpo", &source),
    ]);
    assert!(
        found.iter().any(|rule| rule == "data.write_before_call"),
        "{found:?}"
    );
}

/// Only a request carries a token, and only an answer a slot.
#[test]
fn a_token_comes_in_and_a_slot_goes_out() {
    let refused = |name: &str, source: &str| {
        ids(&[(&format!("domains/pantry/types/{name}.jadpo")[..], source)])
    };
    assert_eq!(
        refused(
            "given_slot",
            "type GivenSlot = Input {\n    slot: UploadSlot\n}\n"
        ),
        ["type.batch_input"]
    );
    assert_eq!(
        refused(
            "kept_slot",
            "type KeptSlot = Output {\n    slot: UploadSlot\n}\n"
        ),
        Vec::<String>::new()
    );
}

/// The runtime answers a store's uploads before any route, so a route at
/// one would never run (area 16, "Uploading").
#[test]
fn a_route_at_a_stores_uploads_is_refused() {
    let route = |path: &str| {
        let routes = format!(
            "use pantry/actions {{ whoever }}\nuse pantry/entities {{ ShelfSheet }}\n\nroutes Files {{\n    prefix: /files\n\n    GET {path} {{\n        authentication: RouteAuthentication.none\n        status: SuccessStatus.no_content\n        run: whoever(link)\n    }}\n}}\n"
        );
        ids(&[
            (
                "domains/pantry/actions/whoever.jadpo",
                "use pantry/entities { ShelfSheet }\n\naction whoever(id: ShelfSheet.id) -> Unit {\n    assert id == id\n}\n",
            ),
            ("domains/pantry/routes/files.jadpo", &routes),
        ])
        .into_iter()
        .filter(|rule| rule.starts_with("route."))
        .collect::<Vec<_>>()
    };
    assert_eq!(
        route("/sheets/uploads/{link: ShelfSheet.id}"),
        ["route.path_overlap"]
    );
    assert_eq!(
        route("/scans/uploads/{link: ShelfSheet.id}"),
        Vec::<String>::new()
    );
}

/// A route asking for a slot reaches the entity's `create` or `update`
/// grant: one that grants neither can never answer.
#[test]
fn a_route_asking_for_a_slot_needs_a_grant() {
    let entity = |lists: &str| {
        format!(
            "entity Bin {{\n    id: Uuid\n    sheet: File {{\n        store: sheets\n        max_size: 1mb\n    }}\n\n    policy: {{\n{lists}\n    }}\n}}\n"
        )
    };
    let reached = |lists: &str| {
        ids(&[
            ("domains/pantry/entities/bin.jadpo", &entity(lists)),
            (
                "domains/pantry/actions/ask.jadpo",
                "use pantry/entities { Bin }\n\naction ask() -> UploadSlot {\n    return Bin.sheet_upload()\n}\n",
            ),
            (
                "domains/pantry/routes/bins.jadpo",
                "use pantry/actions { ask }\n\nroutes Bins {\n    prefix: /bins\n\n    POST /slot {\n        authentication: RouteAuthentication.none\n        output: UploadSlot\n        run: ask()\n    }\n}\n",
            ),
        ])
        .into_iter()
        .filter(|rule| rule != "record.out_of_date")
        .collect::<Vec<_>>()
    };
    assert_eq!(
        reached("        create: [Access.public]\n        read: [Access.public]"),
        Vec::<String>::new()
    );
    assert_eq!(
        reached("        update: [Access.public]\n        read: [Access.public]"),
        Vec::<String>::new()
    );
    assert_eq!(
        reached("        read: [Access.public]"),
        ["policy.effect_ungranted"]
    );
    assert_eq!(
        reached("        create: [Access.authenticated]\n        read: [Access.public]"),
        ["policy.public_grant_missing"]
    );
}

/// A slot, and the write that attaches an upload, each use the field's
/// store, so a deployable that does either has to reach it (area 16,
/// "Uploading"), and it is listed as one that keeps the deployable on
/// one server.
#[test]
fn asking_for_a_slot_or_attaching_reaches_the_fields_store() {
    let reached = |name: &str, action_source: &str, route: &str| {
        let db = jadpo_db::Database::default();
        let routes = format!(
            "use pantry/actions {{ {name} }}\nuse pantry/types {{ SheetUpload }}\n\nroutes Bins {{\n    prefix: /bins\n\n{route}\n}}\n"
        );
        let mut files: Vec<(&str, &str)> = vec![APP];
        files.extend_from_slice(BASE);
        files.push((
            "domains/pantry/types/sheet_upload.jadpo",
            "type SheetUpload = Input {\n    sheet: FileUpload\n}\n",
        ));
        files.push(("domains/pantry/actions/acted.jadpo", action_source));
        files.push(("domains/pantry/routes/bins.jadpo", &routes));
        let project = support::project(&db, Layout::Project, &files);
        let all = jadpo_check::deployables::reaches(&db, project);
        all.first()
            .map(|found| found.connections.keys().cloned().collect::<Vec<String>>())
            .unwrap_or_default()
    };
    let asked = reached(
        "slot_only",
        "use pantry/entities { ShelfSheet }\n\naction slot_only() -> UploadSlot {\n    return ShelfSheet.sheet_upload()\n}\n",
        "    POST /slot {\n        authentication: RouteAuthentication.none\n        output: UploadSlot\n        run: slot_only()\n    }",
    );
    assert_eq!(asked, ["sheets"]);
    let attached = reached(
        "attach_only",
        "use pantry/entities { ShelfSheet }\nuse pantry/types { SheetUpload }\n\naction attach_only(input: SheetUpload) -> Unit {\n    var kept: ShelfSheet = ShelfSheet.create {\n        set: {\n            sheet: input.sheet\n        }\n    }\n    assert kept.id == kept.id\n}\n",
        "    POST / {\n        authentication: RouteAuthentication.none\n        input: SheetUpload\n        status: SuccessStatus.no_content\n        run: attach_only(input)\n    }",
    );
    assert_eq!(attached, ["sheets"]);
}

/// What `app.jadpo` and `config.jadpo` report as rules.
fn connection_rules(app: &str, config: &str) -> Vec<String> {
    support::rules(
        Layout::Project,
        &[("app.jadpo", app), ("config.jadpo", config)],
    )
    .into_iter()
    .filter(|rule| rule != "import.unused" && rule != "config.unused")
    .collect()
}

const S3_CONFIG: &str = "config {\n    files_host: Text\n    files_key_id: Text\n    files_secret: Text {\n        secret: true\n    }\n}\n";

fn s3_app(settings: &str) -> String {
    format!(
        "application {{\n    connections: {{\n        uploads: {{\n            driver: Driver.s3\n{settings}        }}\n    }}\n}}\n"
    )
}

const S3_WHOLE: &str = "            host: config.files_host\n            region: \"auto\"\n            bucket: \"acme\"\n            credential: {\n                key_id: config.files_key_id\n                secret: config.files_secret\n            }\n";

#[test]
fn an_s3_connections_settings_are_held_to_what_they_say() {
    assert_eq!(
        connection_rules(&s3_app(S3_WHOLE), S3_CONFIG),
        Vec::<String>::new()
    );
    // Text written out, and a real address.
    let written = S3_WHOLE.replace(
        "host: config.files_host",
        "host: \"https://acme.r2.cloudflarestorage.com\"",
    );
    assert_eq!(
        connection_rules(&s3_app(&written), S3_CONFIG),
        Vec::<String>::new()
    );

    let missing_region = S3_WHOLE.replace("            region: \"auto\"\n", "");
    assert_eq!(
        connection_rules(&s3_app(&missing_region), S3_CONFIG),
        ["connection.key_missing"]
    );
    let no_credential = "            host: config.files_host\n            region: \"auto\"\n            bucket: \"acme\"\n";
    assert_eq!(
        connection_rules(&s3_app(no_credential), S3_CONFIG),
        ["connection.key_missing"]
    );
    let not_an_address = S3_WHOLE.replace("host: config.files_host", "host: \"localhost:9000\"");
    assert_eq!(
        connection_rules(&s3_app(&not_an_address), S3_CONFIG),
        ["connection.value_invalid"]
    );
    let plain_http = S3_WHOLE.replace("host: config.files_host", "host: \"http://example.com\"");
    assert_eq!(
        connection_rules(&s3_app(&plain_http), S3_CONFIG),
        ["connection.value_invalid"]
    );
    let written_secret =
        S3_WHOLE.replace("secret: config.files_secret", "secret: \"plainly-wrong\"");
    assert_eq!(
        connection_rules(&s3_app(&written_secret), S3_CONFIG),
        ["connection.value_invalid"]
    );
    let not_secret =
        "config {\n    files_host: Text\n    files_key_id: Text\n    files_secret: Text\n}\n";
    assert_eq!(
        connection_rules(&s3_app(S3_WHOLE), not_secret),
        ["connection.value_invalid"]
    );
    let one_part = S3_WHOLE.replace("                key_id: config.files_key_id\n", "");
    assert_eq!(
        connection_rules(&s3_app(&one_part), S3_CONFIG),
        ["connection.key_missing"]
    );
    let extra = format!("{S3_WHOLE}            path: \"/nope\"\n");
    assert_eq!(
        connection_rules(&s3_app(&extra), S3_CONFIG),
        ["connection.key_invalid"]
    );
}

/// A file's run whose actions write no database, in an application with
/// several (area 12, "How a file's run is kept"): kept in the store
/// `events: { store: }` names, as an event of an action that writes none
/// is, and `event.store_required` where the run starts when it names none.
#[test]
fn a_files_run_with_several_databases_and_none_written_is_kept_beside_the_events() {
    let app = |events: &str| {
        format!(
            "application {{\n    connections: {{\n        main: {{\n            driver: Driver.postgres\n        }}\n        ledger: {{\n            driver: Driver.postgres\n        }}\n        sheets: {{\n            driver: Driver.disk\n            path: \"/var/lib/pantry/sheets\"\n        }}\n    }}\n{events}}}\n"
        )
    };
    let read = action(
        "action read_shelves(sheet: File) -> BatchRun {\n    return csv.read(sheet, as: ShelfRow, each: listed -> note_shelf(listed))\n}\n",
    );
    let files = |app: &str| -> Vec<(String, String)> {
        let mut files = vec![("app.jadpo".to_string(), app.to_string())];
        files.extend(
            BASE.iter()
                .map(|(path, text)| (path.to_string(), text.to_string())),
        );
        files.push((
            "domains/pantry/actions/read_shelves.jadpo".to_string(),
            read.clone(),
        ));
        files
    };
    let found = |app: &str| {
        let files = files(app);
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        support::problems(Layout::Project, &borrowed)
            .into_iter()
            .filter(|problem| {
                !problem.contains(" import.unused:")
                    && !problem.contains(" layout.unused_declaration:")
                    && !problem.contains(" migration.")
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        found(&app("")),
        [
            "domains/pantry/actions/read_shelves.jadpo event.store_required: `read_shelves` starts `csv.read` but writes no store to keep its run in"
        ]
    );
    let named = app("    events: {\n        store: ledger\n    }\n");
    assert_eq!(found(&named), Vec::<String>::new());
    // The run's tables are in the store named.
    let files = files(&named);
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, &borrowed);
    assert_eq!(
        jadpo_check::types::file_run_stores(&db, project).get("read_shelves"),
        Some(&Some("ledger".to_string()))
    );
    let schema = jadpo_check::data::schema(&db, project);
    assert!(
        schema.stores["ledger"]
            .tables
            .contains_key("jadpo_batch_runs"),
        "{:?}",
        schema.stores.keys().collect::<Vec<_>>()
    );
}
