//! Exports in the checker (area 19, "Data formats"; area 12, "How an
//! export's run is kept"; chunk 6.4b2): `csv.write`, `tsv.write`,
//! `json_lines.write` and `json.write`'s file form, only in an action,
//! given a store of files, the file's `name:`, `from:` a query's pages and
//! `each:` an inline function giving each item's row, an `Output`; CSV and
//! TSV need `format:`, a `CsvFormat`, and their rows one value a cell;
//! `finished:` receives an `ExportSummary`, whose `file` an entity keeps;
//! what `each:` reads of the action is saved with the run.

use crate::support;

use jadpo_db::Layout;

const BASE: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {\n    connections: {\n        dumps: {\n            driver: Driver.disk\n            path: \"/var/lib/larder/dumps\"\n        }\n    }\n}\n",
    ),
    (
        "domains/larder/types/jar_code.jadpo",
        "type JarCode = Text {\n    min_length: 1\n    max_length: 20\n}\n",
    ),
    (
        "domains/larder/types/jar_page.jadpo",
        "type JarPage = Input {\n    page_size: PageSize {\n        default: 25\n    }\n    after: PageCursor {\n        optional: true\n    }\n}\n",
    ),
    (
        "domains/larder/types/jar_line.jadpo",
        "use larder/types { JarCode }\n\ntype JarLine = Output {\n    code: JarCode {\n        column: \"Jar code\"\n    }\n    tag: JarCode?\n}\n",
    ),
    (
        "domains/larder/entities/jar.jadpo",
        "use larder/types { JarCode }\n\nentity Jar {\n    id: Uuid\n    code: JarCode\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public, Access.system]\n    }\n}\n",
    ),
    (
        "domains/larder/entities/jar_dump.jadpo",
        "entity JarDump {\n    id: Uuid\n    file: File? {\n        store: dumps\n        max_size: 50mb\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public, Access.system]\n        update: [Access.public, Access.system]\n    }\n}\n",
    ),
    (
        "domains/larder/queries/jars.jadpo",
        "use larder/entities { Jar }\nuse larder/types { JarPage }\n\nquery jars(page: JarPage) -> Page<Jar> {\n    from: Jar\n    order: [Jar.id asc]\n    page: {\n        size: page.page_size\n        after: page.after\n    }\n}\n",
    ),
    (
        "domains/larder/actions/dumped.jadpo",
        "use larder/entities { JarDump, JarDumpNotFound }\n\naction dumped(summary: ExportSummary, dump_id: JarDump.id) fails JarDumpNotFound -> Unit {\n    attempt JarDump.update(dump_id) {\n        set: {\n            file: summary.file\n        }\n    }\n}\n",
    ),
];

/// What the base project, with `extra` files besides, reports, but for
/// unused imports and declarations and migrations, which these projects
/// don't keep.
fn ids(extra: &[(&str, &str)]) -> Vec<String> {
    let mut files: Vec<(&str, &str)> = BASE.to_vec();
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

/// An action starting an export with `call`, its parameters a dump's id,
/// a page and a tag.
fn dump(call: &str) -> String {
    dump_with("JarLine", call)
}

/// As [`dump`], importing the row type `row`.
fn dump_with(row: &str, call: &str) -> String {
    format!(
        "use app {{ dumps }}\nuse larder/actions {{ dumped }}\nuse larder/entities {{ JarDump }}\nuse larder/queries {{ jars }}\nuse larder/types {{ JarCode, JarPage, {row} }}\n\naction dump_jars(dump_id: JarDump.id, page: JarPage, tag: JarCode) -> BatchRun {{\n    return {call}\n}}\n"
    )
}

const ROW: &str = "jar -> JarLine {\n            code: jar.code\n            tag: tag\n        }";

fn found(call: &str) -> Vec<String> {
    let action = dump(call);
    ids(&[("domains/larder/actions/dump_jars.jadpo", &action)])
}

#[test]
fn each_writer_checks_with_its_store_name_query_and_row() {
    for call in [
        format!(
            "csv.write(\n        dumps,\n        name: \"jars.csv\",\n        format: CsvFormat.spreadsheet,\n        from: jars(page),\n        each: {ROW},\n        finished: summary -> dumped(summary, dump_id),\n        key: dump_id\n    )"
        ),
        format!(
            "tsv.write(dumps, name: \"jars.tsv\", format: CsvFormat.standard, from: jars(page), each: {ROW})"
        ),
        format!("json_lines.write(dumps, name: \"jars.jsonl\", from: jars(page), each: {ROW})"),
        format!(
            "json.write(dumps, name: \"jars.json\", from: jars(page), each: {ROW}, finished: summary -> dumped(summary, dump_id))"
        ),
    ] {
        assert_eq!(found(&call), Vec::<String>::new(), "{call}");
    }
}

#[test]
fn a_csv_needs_its_format_and_json_takes_none() {
    let missing = found(&format!(
        "csv.write(dumps, name: \"jars.csv\", from: jars(page), each: {ROW})"
    ));
    assert_eq!(missing, ["type.argument_count"]);
    let wrong = found(&format!(
        "tsv.write(dumps, name: \"jars.tsv\", format: \"spreadsheet\", from: jars(page), each: {ROW})"
    ));
    assert_eq!(wrong, ["type.mismatch"]);
    let extra = found(&format!(
        "json_lines.write(dumps, name: \"jars.jsonl\", format: CsvFormat.standard, from: jars(page), each: {ROW})"
    ));
    assert_eq!(extra, ["type.named_argument_unknown"]);
}

#[test]
fn the_store_is_a_store_of_files_and_the_name_text() {
    let database = found(&format!(
        "csv.write(main, name: \"jars.csv\", format: CsvFormat.standard, from: jars(page), each: {ROW})"
    ));
    assert_eq!(database, ["type.mismatch"]);
    let value = found(&format!(
        "csv.write(tag, name: \"jars.csv\", format: CsvFormat.standard, from: jars(page), each: {ROW})"
    ));
    assert_eq!(value, ["type.mismatch"]);
    let name = found(&format!(
        "csv.write(dumps, name: 7, format: CsvFormat.standard, from: jars(page), each: {ROW})"
    ));
    assert_eq!(name, ["type.mismatch"]);
}

#[test]
fn a_row_is_an_output_and_a_csv_cell_one_value() {
    let object = (
        "domains/larder/types/jar_note.jadpo",
        "use larder/types { JarCode }\n\ntype JarNote = Object {\n    code: JarCode\n}\n",
    );
    let not_output = dump_with(
        "JarNote",
        "csv.write(dumps, name: \"jars.csv\", format: CsvFormat.standard, from: jars(page), each: jar -> JarNote {\n        code: jar.code\n    })",
    );
    assert_eq!(
        ids(&[
            object,
            ("domains/larder/actions/dump_jars.jadpo", &not_output)
        ]),
        ["type.batch_row_invalid"]
    );
    let lists = (
        "domains/larder/types/jar_codes.jadpo",
        "use larder/types { JarCode }\n\ntype JarCodes = Output {\n    codes: List<JarCode>\n}\n",
    );
    for (helper, format, expected) in [
        (
            "csv",
            ", format: CsvFormat.standard",
            vec!["type.batch_row_invalid"],
        ),
        (
            "tsv",
            ", format: CsvFormat.standard",
            vec!["type.batch_row_invalid"],
        ),
        ("json_lines", "", vec![]),
        ("json", "", vec![]),
    ] {
        let listed = dump_with(
            "JarCodes",
            &format!(
                "{helper}.write(dumps, name: \"jars\"{format}, from: jars(page), each: jar -> JarCodes {{\n        codes: [jar.code]\n    }})"
            ),
        );
        assert_eq!(
            ids(&[lists, ("domains/larder/actions/dump_jars.jadpo", &listed)]),
            expected,
            "{helper}"
        );
    }
}

#[test]
fn each_is_one_inline_function_and_what_it_reads_is_saved() {
    let not_inline = found(
        "csv.write(dumps, name: \"jars.csv\", format: CsvFormat.standard, from: jars(page), each: tag)",
    );
    assert_eq!(not_inline, ["type.batch_each_invalid"]);
    // A value with no stored form can't be saved with the run: an
    // `Input` the action is given, which the row reads.
    let ask = (
        "domains/larder/types/jar_ask.jadpo",
        "use larder/types { JarCode }\n\ntype JarAsk = Input {\n    code: JarCode\n}\n",
    );
    let unsaved = "use app { dumps }\nuse larder/queries { jars }\nuse larder/types { JarAsk, JarLine, JarPage }\n\naction dump_jars(page: JarPage, ask: JarAsk) -> BatchRun {\n    return csv.write(dumps, name: \"jars.csv\", format: CsvFormat.standard, from: jars(page), each: jar -> JarLine {\n        code: jar.code\n        tag: ask.code\n    })\n}\n";
    assert_eq!(
        ids(&[ask, ("domains/larder/actions/dump_jars.jadpo", unsaved)]),
        ["type.batch_capture_not_storable"]
    );
}

#[test]
fn only_an_action_starts_an_export_and_json_write_in_memory_stays() {
    let function = "use app { dumps }\nuse larder/queries { jars }\nuse larder/types { JarLine, JarPage }\n\nfunction dump_jars(page: JarPage) -> BatchRun {\n    return json_lines.write(dumps, name: \"jars.jsonl\", from: jars(page), each: jar -> JarLine {\n        code: jar.code\n        tag: none\n    })\n}\n";
    assert!(
        ids(&[("domains/larder/libraries/dump_jars.jadpo", function)])
            .contains(&"type.batch_context".to_string())
    );
    let text = "type JarText = Text {}\n";
    let memory = "use larder/types { JarCode, JarLine, JarText }\n\naction line_text(code: JarCode) -> JarText {\n    return JarText(json.write(JarLine {\n        code: code\n        tag: none\n    }))\n}\n";
    assert_eq!(
        ids(&[
            ("domains/larder/types/jar_text.jadpo", text),
            ("domains/larder/actions/line_text.jadpo", memory),
        ]),
        Vec::<String>::new()
    );
}
