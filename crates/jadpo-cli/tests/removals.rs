//! Removing data needs a person (area 5, "Migrations"; area 18,
//! `DataChange`; chunk V3). An entity gone from the source is retired, its
//! table and records kept, and a later migration drops it or a retired
//! column with `jadpo migrate new <purpose> --drop <name>`; an entity moved
//! to another store starts empty there, its records left behind, retired.
//! Each is refused by `jadpo check` until an acceptance the change adds
//! lists it with a person's reason, shows in `jadpo approval` in plain
//! words, and is refused by `jadpo migrate apply` while no acceptance
//! anywhere lists it. A drop lets go of the files its rows hold, and leaves
//! the audit trail's entries alone.

use std::fs;
use std::path::{Path, PathBuf};

use crate::acceptances::{edit, git, jadpo, write};

const TODO: &str = "domains/planning/entities/todo.jadpo";
const TAG: &str = "domains/planning/entities/tag.jadpo";
const FLAGGED: &str = "entity Todo {\n    id: Uuid\n    title: Text\n    legacy_flag: Bool\n}\n";
const UNFLAGGED: &str = "entity Todo {\n    id: Uuid\n    title: Text\n}\n";

fn folder(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("removals-{name}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a folder");
    root
}

/// A project with two entities and its first migration.
fn project(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", "application {}\n");
    write(&root, TODO, FLAGGED);
    write(&root, TAG, "entity Tag {\n    id: Uuid\n}\n");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}", recorded.stdout);
    root
}

/// `project`, committed on `main`, on a branch of its own.
fn tracked(name: &str) -> PathBuf {
    let root = project(name);
    commit(&root, "The project");
    git(&root, &["branch", "-M", "main"]);
    git(&root, &["checkout", "-q", "-b", "change"]);
    root
}

fn commit(root: &Path, message: &str) {
    if !root.join(".git").exists() {
        git(root, &["init", "-q"]);
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", message]);
}

/// Merges branch `from` into `main`, as a pull request does, and starts
/// branch `next`.
fn merged(root: &Path, from: &str, next: &str) {
    commit(root, "The change");
    git(root, &["checkout", "-q", "main"]);
    git(root, &["merge", "-q", "--no-ff", from, "-m", "Merged"]);
    git(root, &["checkout", "-q", "-b", next]);
}

fn migrations(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
}

fn latest(root: &Path, file: &str) -> String {
    let name = migrations(root).pop().expect("a migration");
    fs::read_to_string(root.join(format!("database/migrations/{name}/{file}"))).expect("read")
}

fn set_latest(root: &Path, file: &str, text: &str) {
    let name = migrations(root).pop().expect("a migration");
    fs::write(
        root.join(format!("database/migrations/{name}/{file}")),
        text,
    )
    .expect("write");
}

/// `legacy_flag` retired by a finished migration.
fn retired(root: &Path) {
    write(root, TODO, UNFLAGGED);
    assert_eq!(jadpo(root, &["migrate", "new", "retire_flag"]).code, 0);
    let text = latest(root, "changes.jadpo").replace(
        "Todo.legacy_flag: {}",
        "Todo.legacy_flag: FieldChange.retired",
    );
    set_latest(root, "changes.jadpo", &text);
    let finished = jadpo(root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    assert_eq!(jadpo(root, &["records", "update"]).code, 0);
}

/// `Todo` gone from the source, retired by a finished migration.
fn entity_retired(root: &Path) {
    fs::remove_file(root.join(TODO)).expect("removed");
    assert_eq!(jadpo(root, &["migrate", "new", "remove_todo"]).code, 0);
    let text = latest(root, "changes.jadpo").replace("Todo: {}", "Todo: EntityChange.retired");
    set_latest(root, "changes.jadpo", &text);
    let finished = jadpo(root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    assert_eq!(jadpo(root, &["records", "update"]).code, 0);
}

fn rules(root: &Path) -> Vec<String> {
    let output = jadpo(root, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|d| {
            format!(
                "{}: {}",
                d["rule"].as_str().unwrap_or(""),
                d["message"].as_str().unwrap_or("")
            )
        })
        .collect()
}

/// A drafted acceptance given a person's reason.
fn accept(root: &Path, purpose: &str) -> String {
    let drafted = jadpo(root, &["acceptance", "new", purpose]);
    assert_eq!(drafted.code, 0, "{}{}", drafted.stdout, drafted.stderr);
    let file = fs::read_dir(root.join("acceptances"))
        .expect("acceptances")
        .filter_map(|entry| Some(entry.ok()?.path()))
        .find(|path| path.to_string_lossy().contains(purpose))
        .expect("the draft");
    let text = fs::read_to_string(&file).expect("read");
    fs::write(&file, format!("/// Nothing reads it any more.\n{text}")).expect("write");
    text
}

/// What keeps an entity with a `File` field: a store of files, and the
/// field its rows hold.
const SHEETS_APP: &str = "application {\n    connections: {\n        sheets: {\n            driver: Driver.disk\n            path: \"/var/lib/bakery/sheets\"\n        }\n    }\n}\n";

fn sheet(fields: &str) -> String {
    format!(
        "entity Sheet {{\n    id: Uuid\n{fields}\n    policy: {{\n        read: [Access.public]\n    }}\n}}\n"
    )
}

const SCAN: &str = "    scan: File? {\n        store: sheets\n        max_size: 5mb\n        types: [FileType.csv]\n    }\n";
const COPY: &str = "    copy: File? {\n        store: sheets\n        max_size: 5mb\n        types: [FileType.csv]\n    }\n";
const SHEET: &str = "domains/bakery/entities/sheet.jadpo";

// --- an entity removed --------------------------------------------------

#[test]
fn an_entity_gone_waits_for_its_retirement_and_keeps_its_table() {
    let root = project("retire-entity");
    fs::remove_file(root.join(TODO)).expect("removed");
    let drifted = rules(&root);
    assert!(
        drifted
            .iter()
            .any(|r| r.starts_with("migration.out_of_date")),
        "{drifted:?}"
    );
    let made = jadpo(&root, &["migrate", "new", "remove_todo"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    assert!(
        made.stdout
            .contains("    - table  todo  retired: the table and its records stay"),
        "{}",
        made.stdout
    );
    assert!(
        made.stdout
            .contains("decision: `Todo` is gone: retire its table"),
        "{}",
        made.stdout
    );
    assert!(
        made.stdout
            .contains("next: Write `Todo: EntityChange.retired`"),
        "{}",
        made.stdout
    );
    let changes = latest(&root, "changes.jadpo");
    assert!(changes.contains("    Todo: {}\n"), "{changes}");
    assert!(
        rules(&root)
            .iter()
            .any(|r| r.starts_with("migration.decision_required: ")
                && r.contains("a decision about `Todo`")),
        "{:?}",
        rules(&root)
    );
    set_latest(
        &root,
        "changes.jadpo",
        &changes.replace("Todo: {}", "Todo: FieldChange.retired"),
    );
    assert!(
        rules(&root)
            .iter()
            .any(|r| r.starts_with("migration.decision_invalid: ")),
        "{:?}",
        rules(&root)
    );
    set_latest(
        &root,
        "changes.jadpo",
        &changes.replace("Todo: {}", "Todo: EntityChange.retired"),
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    assert!(
        finished
            .stdout
            .contains("no statements: nothing in the database changes"),
        "{}",
        finished.stdout
    );
    let sql = latest(&root, "postgres.sql");
    assert!(!sql.contains("DROP TABLE"), "{sql}");
    let snapshot: serde_json::Value =
        serde_json::from_str(&latest(&root, "snapshot.json")).expect("JSON");
    assert_eq!(
        snapshot["stores"]["main"]["tables"]["todo"]["retired"],
        serde_json::json!(true),
        "{snapshot}"
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    // Nothing more to migrate, and the screen says what still waits.
    let again = jadpo(&root, &["migrate", "new", "nothing"]);
    assert_eq!(again.code, 0, "{}{}", again.stdout, again.stderr);
    assert!(
        again.stdout.contains("Nothing to migrate")
            && again
                .stdout
                .contains("Todo  retired: its table and records stay")
            && again.stdout.contains("--drop"),
        "{}",
        again.stdout
    );
}

#[test]
fn retiring_an_entity_lets_go_of_what_it_refers_to() {
    let root = project("retire-references");
    write(
        &root,
        "domains/planning/entities/note.jadpo",
        "use planning/entities { Tag }\n\nentity Note {\n    id: Uuid\n    tag_id: Tag.id {\n        on_delete: OnDelete.restricted\n    }\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "add_note"]).code, 0);
    fs::remove_file(root.join("domains/planning/entities/note.jadpo")).expect("removed");
    assert_eq!(jadpo(&root, &["migrate", "new", "remove_note"]).code, 0);
    let text = latest(&root, "changes.jadpo").replace("Note: {}", "Note: EntityChange.retired");
    set_latest(&root, "changes.jadpo", &text);
    assert_eq!(jadpo(&root, &["migrate", "new"]).code, 0);
    let sql = latest(&root, "postgres.sql");
    // Its records no longer hold `Tag`s back from being deleted.
    assert!(
        sql.contains("ALTER TABLE \"note\" DROP CONSTRAINT \"note_tag_id_fkey\""),
        "{sql}"
    );
    assert!(!sql.contains("DROP TABLE"), "{sql}");
}

#[test]
fn a_retired_table_cant_come_back_until_it_is_dropped() {
    let root = project("retired-comes-back");
    entity_retired(&root);
    write(&root, TODO, UNFLAGGED);
    let refused = jadpo(&root, &["migrate", "new", "bring_back"]);
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(
        refused.stderr.contains("rule: toolchain.not_built_yet")
            && refused.stderr.contains("retired table `todo` comes back"),
        "{}",
        refused.stderr
    );
}

// --- what a store no longer needs stays ---------------------------------

#[test]
fn a_system_table_nothing_asks_for_any_more_stays() {
    let root = folder("system-stays");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        TODO,
        "entity Todo {\n    id: Uuid\n\n    audit: {}\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    assert!(
        latest(&root, "snapshot.json").contains("\"jadpo_audit\""),
        "the history's table"
    );
    write(&root, TODO, "entity Todo {\n    id: Uuid\n}\n");
    let again = jadpo(&root, &["migrate", "new", "unaudit"]);
    assert_eq!(again.code, 0, "{}{}", again.stdout, again.stderr);
    assert!(
        again.stdout.contains("Nothing to migrate"),
        "{}",
        again.stdout
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
}

// --- dropping -----------------------------------------------------------

#[test]
fn a_retired_column_is_dropped_by_name_and_shows_exactly_what_goes() {
    let root = project("drop-column");
    retired(&root);
    let made = jadpo(
        &root,
        &["migrate", "new", "drop_flag", "--drop", "Todo.legacy_flag"],
    );
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    assert!(
        made.stdout
            .contains("    - legacy_flag  boolean  dropped: the column and its data go for good"),
        "{}",
        made.stdout
    );
    assert!(
        made.stdout.contains("1 statement: drop column"),
        "{}",
        made.stdout
    );
    assert!(
        made.stdout
            .contains("decision: 1 change waits for a person's acceptance"),
        "{}",
        made.stdout
    );
    assert!(
        made.stdout.contains("`jadpo acceptance new <purpose>`"),
        "{}",
        made.stdout
    );
    assert!(
        latest(&root, "postgres.sql").contains("ALTER TABLE \"todo\" DROP COLUMN \"legacy_flag\";"),
        "{}",
        latest(&root, "postgres.sql")
    );
    let changes = latest(&root, "changes.jadpo");
    assert!(
        changes.contains("    Todo.legacy_flag: FieldChange.dropped\n"),
        "{changes}"
    );
    assert!(
        !latest(&root, "snapshot.json").contains("legacy_flag"),
        "the column is gone from the snapshot"
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    // Nothing else disagrees: only a person's acceptance is left.
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    // Running it again finds nothing to finish.
    let again = jadpo(&root, &["migrate", "new", "again"]);
    assert!(
        again.stdout.contains("Nothing to migrate"),
        "{}",
        again.stdout
    );
    // The line that decides it can't be taken out.
    set_latest(
        &root,
        "changes.jadpo",
        &changes.replace("    Todo.legacy_flag: FieldChange.dropped\n", ""),
    );
    assert!(
        rules(&root)
            .iter()
            .any(|r| r.starts_with("migration.decision_required: ")
                && r.contains("`Todo.legacy_flag`")),
        "{:?}",
        rules(&root)
    );
    set_latest(
        &root,
        "changes.jadpo",
        &changes.replace("FieldChange.dropped", "FieldChange.retired"),
    );
    assert!(
        rules(&root)
            .iter()
            .any(|r| r.starts_with("migration.decision_invalid: ")),
        "{:?}",
        rules(&root)
    );
}

#[test]
fn a_retired_entity_is_dropped_by_name() {
    let root = project("drop-table");
    entity_retired(&root);
    let made = jadpo(&root, &["migrate", "new", "drop_todo", "--drop", "Todo"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    assert!(
        made.stdout
            .contains("    - table  todo  dropped: the table and its records go for good"),
        "{}",
        made.stdout
    );
    assert!(
        latest(&root, "postgres.sql").contains("DROP TABLE \"todo\";"),
        "{}",
        latest(&root, "postgres.sql")
    );
    assert!(
        latest(&root, "changes.jadpo").contains("    Todo: EntityChange.dropped\n"),
        "{}",
        latest(&root, "changes.jadpo")
    );
    assert!(!latest(&root, "snapshot.json").contains("\"todo\""));
    // Now the name is free, and an entity may take it.
    write(&root, TODO, UNFLAGGED);
    let added = jadpo(&root, &["migrate", "new", "add_todo"]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    assert!(latest(&root, "postgres.sql").contains("CREATE TABLE \"todo\""));
}

#[test]
fn one_thing_is_dropped_at_a_time() {
    let root = project("drop-one");
    retired(&root);
    entity_retired(&root);
    let refused = jadpo(
        &root,
        &[
            "migrate",
            "new",
            "drop_both",
            "--drop",
            "Todo.legacy_flag",
            "--drop",
            "Todo",
        ],
    );
    assert_eq!(refused.code, 2, "{}{}", refused.stdout, refused.stderr);
    assert!(
        refused.stderr.contains("`--drop` is given more than once"),
        "{}",
        refused.stderr
    );
}

#[test]
fn only_what_is_retired_can_be_dropped() {
    let root = project("drop-refused");
    let refused = |root: &Path, args: &[&str], said: &str| {
        let output = jadpo(root, args);
        assert_eq!(output.code, 2, "{}{}", output.stdout, output.stderr);
        assert!(
            output.stderr.contains("rule: cli.migrate_arguments") && output.stderr.contains(said),
            "{}",
            output.stderr
        );
    };
    let before = migrations(&root);
    refused(
        &root,
        &["migrate", "new", "drop_it", "--drop", "Todo.title"],
        "`Todo.title` isn't retired",
    );
    refused(
        &root,
        &["migrate", "new", "drop_it", "--drop", "Todo"],
        "`Todo` isn't retired",
    );
    refused(
        &root,
        &["migrate", "new", "drop_it", "--drop", "Nope.nothing"],
        "`Nope.nothing` is no retired field or entity",
    );
    refused(
        &root,
        &["migrate", "apply", "--drop", "Todo"],
        "`--drop` belongs to `jadpo migrate new`",
    );
    // While the latest migration waits for a decision, it is finished first.
    write(&root, TODO, UNFLAGGED);
    assert_eq!(jadpo(&root, &["migrate", "new", "retire_flag"]).code, 0);
    refused(
        &root,
        &["migrate", "new", "--drop", "Todo.legacy_flag"],
        "waits for its decisions",
    );
    assert_eq!(migrations(&root).len(), before.len() + 1);
}

// --- moving -------------------------------------------------------------

const TWO_STORES: &str = "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        archive: {\n            driver: Driver.postgres\n        }\n    }\n}\n";

fn two_stores(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", TWO_STORES);
    write(
        &root,
        TODO,
        "entity Todo {\n    id: Uuid\n    title: Text\n\n    store: main\n}\n",
    );
    write(
        &root,
        TAG,
        "entity Tag {\n    id: Uuid\n\n    store: archive\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    root
}

#[test]
fn an_entity_moved_starts_empty_and_leaves_its_records_behind() {
    let root = two_stores("move");
    write(
        &root,
        TODO,
        "entity Todo {\n    id: Uuid\n    title: Text\n\n    store: archive\n}\n",
    );
    let made = jadpo(&root, &["migrate", "new", "move_todo"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    assert!(
        made.stdout
            .contains("    + table  todo  in `archive`: new and empty"),
        "{}",
        made.stdout
    );
    assert!(
        made.stdout
            .contains("    - table  todo  in `main`: retired, its records stay and aren't copied"),
        "{}",
        made.stdout
    );
    // No decision to write: the acceptance is the decision.
    assert!(!made.stdout.contains("changes.jadpo"), "{}", made.stdout);
    assert!(
        made.stdout
            .contains("decision: 1 change waits for a person's acceptance"),
        "{}",
        made.stdout
    );
    let sql = latest(&root, "postgres.sql");
    assert!(
        sql.contains("-- store: archive\nCREATE TABLE \"todo\""),
        "{sql}"
    );
    assert!(!sql.contains("DROP TABLE"), "{sql}");
    let snapshot: serde_json::Value =
        serde_json::from_str(&latest(&root, "snapshot.json")).expect("JSON");
    assert_eq!(
        snapshot["stores"]["main"]["tables"]["todo"]["retired"],
        serde_json::json!(true)
    );
    assert_eq!(
        snapshot["stores"]["archive"]["tables"]["todo"]["retired"],
        serde_json::Value::Null
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    // What was left behind is dropped by its name, a later decision of
    // its own; the table in the new store stays.
    let dropped = jadpo(&root, &["migrate", "new", "drop_old", "--drop", "Todo"]);
    assert_eq!(dropped.code, 0, "{}{}", dropped.stdout, dropped.stderr);
    let sql = latest(&root, "postgres.sql");
    assert!(
        sql.contains("-- store: main\nDROP TABLE \"todo\";"),
        "{sql}"
    );
    assert!(!sql.contains("store: archive"), "{sql}");
}

// --- encrypted fields ------------------------------------------------------

const NOTE: &str = "domains/clinic/entities/note.jadpo";

fn note(fields: &str, store: &str) -> String {
    format!(
        "entity Note {{\n    id: Uuid\n{fields}\n    store: {store}\n\n    policy: {{\n        read: [Access.public]\n    }}\n}}\n"
    )
}

const SECRET: &str = "    secret: Text {\n        encrypted: true\n    }\n";

/// A project whose `Note` holds a required sealed field, in `main` of two
/// stores.
fn sealed(name: &str) -> PathBuf {
    let root = folder(name);
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        archive: {\n            driver: Driver.postgres\n        }\n    }\n    schedules: {\n        store: main\n    }\n    field_encryption_key: config.field_encryption_key\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    field_encryption_key: EncryptionKey\n}\n",
    );
    write(&root, NOTE, &note(SECRET, "main"));
    write(
        &root,
        TAG,
        "entity Tag {\n    id: Uuid\n\n    store: archive\n}\n",
    );
    let made = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    root
}

/// A sealed field is retired and dropped as any other is: neither reads or
/// writes what it holds, so neither is what `encrypted:`'s refusals are
/// about (area 15, "Encrypted fields"), and they still stand.
#[test]
fn a_sealed_field_is_retired_and_dropped_as_any_other() {
    let root = sealed("sealed-field");
    // Those refusals, with the field still in use.
    write(&root, NOTE, &note("    secret: Text\n", "main"));
    let off = jadpo(&root, &["migrate", "new", "unseal"]);
    assert_eq!(off.code, 1, "{}{}", off.stdout, off.stderr);
    assert!(
        off.stderr.contains("turns `encrypted:` off"),
        "{}",
        off.stderr
    );
    write(
        &root,
        NOTE,
        &note(
            &format!("{SECRET}    other: Text {{\n        encrypted: true\n    }}\n"),
            "main",
        ),
    );
    let required = jadpo(&root, &["migrate", "new", "add_other"]);
    assert_eq!(required.code, 1, "{}{}", required.stdout, required.stderr);
    assert!(
        required.stderr.contains("required sealed column"),
        "{}",
        required.stderr
    );

    // Retired.
    write(&root, NOTE, &note("", "main"));
    let made = jadpo(&root, &["migrate", "new", "retire_secret"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    let text = latest(&root, "changes.jadpo")
        .replace("Note.secret: {}", "Note.secret: FieldChange.retired");
    set_latest(&root, "changes.jadpo", &text);
    let finished = jadpo(&root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    let sql = latest(&root, "postgres.sql");
    assert!(
        sql.contains("ALTER COLUMN \"secret\" DROP NOT NULL"),
        "{sql}"
    );
    let snapshot: serde_json::Value =
        serde_json::from_str(&latest(&root, "snapshot.json")).expect("JSON");
    let column = &snapshot["stores"]["main"]["tables"]["note"]["columns"]["secret"];
    assert_eq!(column["retired"], serde_json::json!(true), "{column}");
    assert_eq!(column["sealed"], serde_json::json!(true), "{column}");
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    assert_eq!(jadpo(&root, &["check"]).code, 0);

    // Back as a plain field, it would read sealed bytes as text.
    write(&root, NOTE, &note("    secret: Text?\n", "main"));
    let back = jadpo(&root, &["migrate", "new", "bring_back"]);
    assert_eq!(back.code, 1, "{}{}", back.stdout, back.stderr);
    write(&root, NOTE, &note("", "main"));

    // Dropped, for good, with what it held shown as what the column is.
    let dropped = jadpo(
        &root,
        &["migrate", "new", "drop_secret", "--drop", "Note.secret"],
    );
    assert_eq!(dropped.code, 0, "{}{}", dropped.stdout, dropped.stderr);
    assert!(
        dropped.stdout.contains("- secret  bytea  dropped"),
        "{}",
        dropped.stdout
    );
    assert!(
        latest(&root, "postgres.sql").contains("ALTER TABLE \"note\" DROP COLUMN \"secret\";"),
        "{}",
        latest(&root, "postgres.sql")
    );
    // A later required sealed field of that name is new, so it is refused
    // for the reason any is.
    write(&root, NOTE, &note(SECRET, "main"));
    let again = jadpo(&root, &["migrate", "new", "add_secret"]);
    assert_eq!(again.code, 1, "{}{}", again.stdout, again.stderr);
    assert!(
        again.stderr.contains("required sealed column"),
        "{}",
        again.stderr
    );
}

/// A required sealed field is no reason to refuse a move or a retirement:
/// a table made new has no rows to seal, and the one left behind is kept as
/// it is.
#[test]
fn an_entity_with_a_sealed_field_moves_and_retires_as_any_other() {
    let root = sealed("sealed-entity");
    write(&root, NOTE, &note(SECRET, "archive"));
    let moved = jadpo(&root, &["migrate", "new", "move_note"]);
    assert_eq!(moved.code, 0, "{}{}", moved.stdout, moved.stderr);
    let sql = latest(&root, "postgres.sql");
    assert!(sql.contains("CREATE TABLE \"note\""), "{sql}");
    let snapshot: serde_json::Value =
        serde_json::from_str(&latest(&root, "snapshot.json")).expect("JSON");
    let left = &snapshot["stores"]["main"]["tables"]["note"];
    assert_eq!(left["retired"], serde_json::json!(true), "{left}");
    assert_eq!(
        left["columns"]["secret"]["sealed"],
        serde_json::json!(true),
        "{left}"
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    let dropped = jadpo(&root, &["migrate", "new", "drop_old", "--drop", "Note"]);
    assert_eq!(dropped.code, 0, "{}{}", dropped.stdout, dropped.stderr);
    assert!(
        latest(&root, "postgres.sql").contains("-- store: main\nDROP TABLE \"note\";"),
        "{}",
        latest(&root, "postgres.sql")
    );

    fs::remove_file(root.join(NOTE)).expect("removed");
    let made = jadpo(&root, &["migrate", "new", "remove_note"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    assert!(
        latest(&root, "changes.jadpo").contains("    Note: {}\n"),
        "{}",
        latest(&root, "changes.jadpo")
    );
}

// --- the acceptance --------------------------------------------------------

#[test]
fn check_refuses_each_removal_until_an_acceptance_names_it() {
    let root = tracked("gate");
    // The field's retirement needs no acceptance: nothing is out of reach.
    retired(&root);
    assert_eq!(rules(&root), Vec::<String>::new());
    merged(&root, "change", "drop-column");

    let made = jadpo(
        &root,
        &["migrate", "new", "drop_flag", "--drop", "Todo.legacy_flag"],
    );
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let output = jadpo(&root, &["check"]);
    assert_eq!(output.code, 1, "{}{}", output.stdout, output.stderr);
    let text = format!("{}{}", output.stdout, output.stderr);
    assert!(
        text.contains("rule: acceptance.missing")
            && text.contains("dropping a column removes its data for good: Todo.legacy_flag")
            && text.contains("DataChange.column_dropped")
            && text.contains("changes.jadpo:"),
        "{text}"
    );
    // A build stops as the check does.
    assert_eq!(
        rules(&root),
        ["acceptance.missing: dropping a column removes its data for good: Todo.legacy_flag"]
    );
    let draft = accept(&root, "drop_flag");
    assert!(
        draft.contains("Todo.legacy_flag: DataChange.column_dropped"),
        "{draft}"
    );
    assert_eq!(rules(&root), Vec::<String>::new());
    merged(&root, "drop-column", "remove-entity");

    // An acceptance from before this change allows nothing new.
    entity_retired(&root);
    assert_eq!(
        rules(&root),
        ["acceptance.missing: removing an entity takes its records out of the app's reach: Todo"]
    );
    let draft = accept(&root, "remove_todo");
    assert!(draft.contains("Todo: DataChange.entity_removed"), "{draft}");
    assert_eq!(rules(&root), Vec::<String>::new());
    merged(&root, "remove-entity", "drop-table");

    assert_eq!(
        jadpo(&root, &["migrate", "new", "drop_todo", "--drop", "Todo"]).code,
        0
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    assert_eq!(
        rules(&root),
        ["acceptance.missing: dropping a table removes its records for good: Todo"]
    );
    // A draft with no reason doesn't count.
    let drafted = jadpo(&root, &["acceptance", "new", "drop_todo"]);
    assert_eq!(drafted.code, 0, "{}{}", drafted.stdout, drafted.stderr);
    assert!(
        rules(&root)
            .iter()
            .any(|r| r.starts_with("acceptance.no_reason: ")),
        "{:?}",
        rules(&root)
    );
    let file = fs::read_dir(root.join("acceptances"))
        .expect("acceptances")
        .filter_map(|entry| Some(entry.ok()?.path()))
        .find(|path| path.to_string_lossy().contains("drop_todo"))
        .expect("the draft");
    let text = fs::read_to_string(&file).expect("read");
    fs::write(&file, format!("/// Nothing reads it any more.\n{text}")).expect("write");
    assert_eq!(rules(&root), Vec::<String>::new());
    // An entry naming nothing this change does is refused.
    let extra = text.replace(
        "Todo: DataChange.table_dropped",
        "Todo: DataChange.table_dropped\n        Tag: DataChange.table_dropped",
    );
    fs::write(&file, format!("/// Nothing reads it any more.\n{extra}")).expect("write");
    assert!(
        rules(&root)
            .iter()
            .any(|r| r.starts_with("acceptance.unmatched: ")),
        "{:?}",
        rules(&root)
    );
}

#[test]
fn moving_an_entity_needs_an_acceptance_too() {
    let root = two_stores("gate-move");
    commit(&root, "Two stores");
    git(&root, &["branch", "-M", "main"]);
    git(&root, &["checkout", "-q", "-b", "move"]);
    write(
        &root,
        TODO,
        "entity Todo {\n    id: Uuid\n    title: Text\n\n    store: archive\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "move_todo"]).code, 0);
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    assert_eq!(
        rules(&root),
        ["acceptance.missing: moving an entity to another store leaves its records behind: Todo"]
    );
    let draft = accept(&root, "move_todo");
    assert!(draft.contains("Todo: DataChange.entity_moved"), "{draft}");
    assert_eq!(rules(&root), Vec::<String>::new());
}

#[test]
fn a_migration_already_merged_cant_be_edited_into_a_removal() {
    let root = tracked("edited");
    retired(&root);
    merged(&root, "change", "edit");
    // The retiring migration, on the before side, edited to drop the column
    // too, with no new folder for the change to be seen in.
    let mut schema =
        jadpo_graph::schema::Schema::read(&latest(&root, "snapshot.json")).expect("read");
    schema
        .stores
        .get_mut("main")
        .and_then(|store| store.tables.get_mut("todo"))
        .expect("the table")
        .columns
        .remove("legacy_flag");
    set_latest(&root, "snapshot.json", &schema.to_document());
    let sql = latest(&root, "postgres.sql");
    set_latest(
        &root,
        "postgres.sql",
        &format!("{sql}ALTER TABLE \"todo\" DROP COLUMN \"legacy_flag\";\n"),
    );
    let found = rules(&root);
    assert!(
        found.iter().any(|r| r
            == "acceptance.missing: dropping a column removes its data for good: Todo.legacy_flag"),
        "{found:?}"
    );
}

#[test]
fn a_hand_written_drop_in_the_sql_is_refused() {
    let root = project("hand-written");
    retired(&root);
    assert_eq!(jadpo(&root, &["migrate", "new", "add_nothing"]).code, 0);
    write(
        &root,
        TODO,
        "entity Todo {\n    id: Uuid\n    title: Text\n    note: Text?\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "add_note"]).code, 0);
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    assert_eq!(rules(&root), Vec::<String>::new());
    // The migration before the latest, whose SQL `check` doesn't otherwise
    // read, gets a `DROP` nobody decided.
    let names = migrations(&root);
    let earlier = root.join(format!(
        "database/migrations/{}/postgres.sql",
        names[names.len() - 2]
    ));
    let sql = fs::read_to_string(&earlier).expect("read");
    fs::write(
        &earlier,
        format!("{sql}ALTER TABLE \"todo\" DROP COLUMN \"title\";\n"),
    )
    .expect("write");
    let found = rules(&root);
    assert!(
        found
            .iter()
            .any(|r| r.starts_with("migration.record_invalid: ")
                && r.contains("drops what its snapshot keeps")),
        "{found:?}"
    );
}

// --- the approval report -------------------------------------------------

#[test]
fn the_approval_report_says_what_goes_in_plain_words() {
    let root = tracked("approval");
    entity_retired(&root);
    let output = jadpo(&root, &["approval", "--json"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    let decisions = &document["data"]["decisions"];
    assert_eq!(decisions[0]["subject"], "Todo", "{decisions}");
    assert_eq!(decisions[0]["change"], "DataChange.entity_removed");
    assert_eq!(decisions[0]["acceptance"], "missing");
    assert_eq!(
        decisions[0]["said"],
        "`Todo` leaves the app: table `todo` and its records stay in store `main`, retired, where erasure and retention no longer reach them, until a later migration drops them"
    );
    accept(&root, "remove_todo");
    merged(&root, "change", "drop");
    assert_eq!(
        jadpo(&root, &["migrate", "new", "drop_todo", "--drop", "Todo"]).code,
        0
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let output = jadpo(&root, &["approval", "--json"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    let decisions = &document["data"]["decisions"];
    assert_eq!(decisions[0]["change"], "DataChange.table_dropped");
    assert_eq!(
        decisions[0]["said"],
        "`Todo` is dropped for good from store `main`: table `todo`, every record in it and its columns `id`, `title` and `legacy_flag`"
    );
    let screen = jadpo(&root, &["approval"]);
    assert!(
        screen.stdout.contains("`Todo` is dropped for good"),
        "{}",
        screen.stdout
    );
    let report = fs::read_to_string(root.join("build/review/approval.md")).expect("report");
    assert!(report.contains("`Todo` is dropped for good"), "{report}");
    assert!(report.contains("changes.jadpo"), "{report}");
}

// --- applying -------------------------------------------------------------

#[test]
fn apply_refuses_a_drop_nobody_accepted_before_it_reaches_a_database() {
    let root = project("apply-refused");
    retired(&root);
    assert_eq!(
        jadpo(
            &root,
            &["migrate", "new", "drop_flag", "--drop", "Todo.legacy_flag"]
        )
        .code,
        0
    );
    // No database listens here: the refusal comes first.
    let refused = jadpo(
        &root,
        &["migrate", "apply", "--url", "postgresql://localhost:1/none"],
    );
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    let text = format!("{}{}", refused.stdout, refused.stderr);
    assert!(
        text.contains("rule: migration.drop_not_accepted")
            && text.contains("Todo.legacy_flag: DataChange.column_dropped"),
        "{text}"
    );
    // A draft with no reason isn't an acceptance.
    write(
        &root,
        "acceptances/20261020T1500_drop_flag.jadpo",
        "acceptance {\n    changes: {\n        Todo.legacy_flag: DataChange.column_dropped\n    }\n}\n",
    );
    let refused = jadpo(
        &root,
        &["migrate", "apply", "--url", "postgresql://localhost:1/none"],
    );
    let text = format!("{}{}", refused.stdout, refused.stderr);
    assert!(text.contains("rule: migration.drop_not_accepted"), "{text}");
    // With a reason, it goes on to the database.
    write(
        &root,
        "acceptances/20261020T1500_drop_flag.jadpo",
        "/// Nothing reads it any more.\nacceptance {\n    changes: {\n        Todo.legacy_flag: DataChange.column_dropped\n    }\n}\n",
    );
    let reached = jadpo(
        &root,
        &["migrate", "apply", "--url", "postgresql://localhost:1/none"],
    );
    let text = format!("{}{}", reached.stdout, reached.stderr);
    assert!(!text.contains("migration.drop_not_accepted"), "{text}");
    assert!(text.contains("rule: migration.apply_failed"), "{text}");
}

#[test]
fn outside_git_check_judges_a_drop_all_the_same() {
    let root = std::env::temp_dir().join(format!("jadpo-removals-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a folder");
    write(&root, "app.jadpo", "application {}\n");
    write(&root, TODO, FLAGGED);
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    assert_eq!(rules(&root), Vec::<String>::new());
    retired(&root);
    assert_eq!(
        jadpo(
            &root,
            &["migrate", "new", "drop_flag", "--drop", "Todo.legacy_flag"]
        )
        .code,
        0
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let found = rules(&root);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("migration.drop_not_accepted: "),
        "{found:?}"
    );
    write(
        &root,
        "acceptances/20261020T1500_drop_flag.jadpo",
        "/// Nothing reads it any more.\nacceptance {\n    changes: {\n        Todo.legacy_flag: DataChange.column_dropped\n    }\n}\n",
    );
    assert_eq!(rules(&root), Vec::<String>::new());
    let _ = fs::remove_dir_all(&root);
}

/// Runs `sql` on the database at `url` and gives each row's columns as text.
fn rows(url: &str, sql: &str) -> Vec<Vec<String>> {
    use jadpo_runtime::{Database, SqlValue};
    let store = jadpo_host_native::postgres(url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        let rows = transaction.query(sql, &[]).await.expect("query");
        transaction.commit().await.expect("commit");
        rows.into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|value| match value {
                        SqlValue::Text(text) => text,
                        SqlValue::Int4(n) => n.to_string(),
                        SqlValue::Int8(n) => n.to_string(),
                        SqlValue::Null(_) => "null".to_string(),
                        other => format!("{other:?}"),
                    })
                    .collect()
            })
            .collect()
    })
}

fn file_json(key: &str) -> String {
    format!(
        "'{{\"store\":\"sheets\",\"key\":\"{key}\",\"name\":\"a.csv\",\"size\":1,\"type\":\"text/csv\"}}'::jsonb"
    )
}

fn accept_by_hand(root: &Path, name: &str, entries: &str) {
    write(
        root,
        &format!("acceptances/{name}.jadpo"),
        &format!(
            "/// Nothing reads it any more.\nacceptance {{\n    changes: {{\n{entries}\n    }}\n}}\n"
        ),
    );
}

#[test]
fn a_drop_lets_go_of_the_files_its_rows_hold() {
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("removals_files").expect("database");
    let root = folder("files");
    write(&root, "app.jadpo", SHEETS_APP);
    write(&root, SHEET, &sheet(&format!("{SCAN}{COPY}")));
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    // Rows hold `a` in `scan` and `copy`, `b` in `scan` and `c` in `copy`.
    rows(
        &url,
        &format!(
            "INSERT INTO \"sheet\" (\"id\", \"scan\", \"copy\") VALUES \
             (gen_random_uuid(), {a}, {a}), (gen_random_uuid(), {b}, {c}), \
             (gen_random_uuid(), NULL, NULL)",
            a = file_json("a"),
            b = file_json("b"),
            c = file_json("c")
        ),
    );
    let held = |url: &str| {
        rows(
            url,
            "SELECT \"key\", \"holders\" FROM \"jadpo_files\" ORDER BY \"key\"",
        )
    };
    assert_eq!(
        held(&url),
        [["a", "2"], ["b", "1"], ["c", "1"]],
        "the trigger counts them"
    );
    // `scan` is retired: still counted, nothing lets go.
    write(&root, SHEET, &sheet(COPY));
    assert_eq!(jadpo(&root, &["migrate", "new", "retire_scan"]).code, 0);
    let text =
        latest(&root, "changes.jadpo").replace("Sheet.scan: {}", "Sheet.scan: FieldChange.retired");
    set_latest(&root, "changes.jadpo", &text);
    assert_eq!(jadpo(&root, &["migrate", "new"]).code, 0);
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert_eq!(held(&url), [["a", "2"], ["b", "1"], ["c", "1"]]);

    // Dropping `scan` lets go of what it holds; `copy` still holds `a`.
    assert_eq!(
        jadpo(
            &root,
            &["migrate", "new", "drop_scan", "--drop", "Sheet.scan"]
        )
        .code,
        0
    );
    let sql = latest(&root, "postgres.sql");
    let release = sql
        .find("UPDATE \"jadpo_files\"")
        .expect("lets go of its files");
    let dropped = sql.find("DROP COLUMN \"scan\"").expect("drops the column");
    assert!(release < dropped, "{sql}");
    accept_by_hand(
        &root,
        "20261020T1500_drop_scan",
        "        Sheet.scan: DataChange.column_dropped",
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert!(
        applied.stdout.contains("Sheet.scan") && applied.stdout.contains("dropped"),
        "{}",
        applied.stdout
    );
    assert_eq!(held(&url), [["a", "1"], ["b", "0"], ["c", "1"]]);
    // The trigger still counts `copy`.
    rows(
        &url,
        &format!(
            "UPDATE \"sheet\" SET \"copy\" = {d} WHERE \"copy\"->>'key' = 'c'",
            d = file_json("d")
        ),
    );
    assert_eq!(held(&url), [["a", "1"], ["b", "0"], ["c", "0"], ["d", "1"]]);

    // The entity retired, then dropped: its table's rows let go.
    fs::remove_file(root.join(SHEET)).expect("removed");
    write(&root, TAG, "entity Tag {\n    id: Uuid\n}\n");
    assert_eq!(jadpo(&root, &["migrate", "new", "remove_sheet"]).code, 0);
    let text = latest(&root, "changes.jadpo").replace("Sheet: {}", "Sheet: EntityChange.retired");
    set_latest(&root, "changes.jadpo", &text);
    assert_eq!(jadpo(&root, &["migrate", "new"]).code, 0);
    accept_by_hand(
        &root,
        "20261020T1501_remove_sheet",
        "        Sheet: DataChange.entity_removed",
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert_eq!(
        held(&url),
        [["a", "1"], ["b", "0"], ["c", "0"], ["d", "1"]],
        "a retired table's rows still hold their files"
    );
    assert_eq!(
        jadpo(&root, &["migrate", "new", "drop_sheet", "--drop", "Sheet"]).code,
        0
    );
    accept_by_hand(
        &root,
        "20261020T1502_drop_sheet",
        "        Sheet: DataChange.table_dropped",
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert_eq!(held(&url), [["a", "0"], ["b", "0"], ["c", "0"], ["d", "0"]]);
    // Released with nothing to say it has been dealt with, so the tick
    // finds each.
    assert_eq!(
        rows(
            &url,
            "SELECT count(*) FROM \"jadpo_files\" WHERE \"since\" IS NOT NULL"
        ),
        [["0"]]
    );
    assert_eq!(
        rows(
            &url,
            "SELECT count(*) FROM information_schema.tables WHERE table_name = 'sheet'"
        ),
        [["0"]]
    );
}

#[test]
fn a_drop_leaves_the_audit_trail_alone() {
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("removals_audit").expect("database");
    let root = folder("audit");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        TODO,
        "entity Todo {\n    id: Uuid\n    title: Text\n\n    audit: {}\n}\n",
    );
    write(&root, TAG, "entity Tag {\n    id: Uuid\n}\n");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    let entries = |url: &str| rows(url, "SELECT count(*) FROM \"jadpo_audit\"");
    // One entry of the record's history.
    rows(
        &url,
        "INSERT INTO \"jadpo_audit\" (\"entity\", \"record_id\", \"seq\", \"at\", \"change\", \"by\", \"changes\", \"encoding\") \
         VALUES ('Todo', gen_random_uuid(), 1, now(), 'created', 'no_caller', '[]', 1)",
    );
    assert_eq!(entries(&url), [["1"]]);

    fs::remove_file(root.join(TODO)).expect("removed");
    assert_eq!(jadpo(&root, &["migrate", "new", "remove_todo"]).code, 0);
    let text = latest(&root, "changes.jadpo").replace("Todo: {}", "Todo: EntityChange.retired");
    set_latest(&root, "changes.jadpo", &text);
    assert_eq!(jadpo(&root, &["migrate", "new"]).code, 0);
    assert!(
        latest(&root, "snapshot.json").contains("\"jadpo_audit\""),
        "the history's table stays once no entity is audited"
    );
    accept_by_hand(
        &root,
        "20261020T1500_remove_todo",
        "        Todo: DataChange.entity_removed",
    );
    assert_eq!(
        jadpo(&root, &["migrate", "new", "drop_todo", "--drop", "Todo"]).code,
        0
    );
    let sql = latest(&root, "postgres.sql");
    assert!(!sql.contains("jadpo_audit"), "{sql}");
    accept_by_hand(
        &root,
        "20261020T1501_drop_todo",
        "        Todo: DataChange.table_dropped",
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert_eq!(entries(&url), [["1"]], "the history stays");
    assert_eq!(
        rows(
            &url,
            "SELECT count(*) FROM information_schema.tables WHERE table_name = 'todo'"
        ),
        [["0"]]
    );
}

#[test]
fn a_field_comes_back_only_after_its_drop() {
    let root = project("field-comes-back");
    retired(&root);
    edit(
        &root,
        TODO,
        "    title: Text\n",
        "    title: Text\n    legacy_flag: Bool?\n",
    );
    let refused = jadpo(&root, &["migrate", "new", "bring_back"]);
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(
        refused
            .stderr
            .contains("retired column `todo.legacy_flag` comes back")
            && refused.stderr.contains("--drop"),
        "{}",
        refused.stderr
    );
    write(&root, TODO, UNFLAGGED);
    assert_eq!(
        jadpo(
            &root,
            &["migrate", "new", "drop_flag", "--drop", "Todo.legacy_flag"]
        )
        .code,
        0
    );
    edit(
        &root,
        TODO,
        "    title: Text\n",
        "    title: Text\n    legacy_flag: Bool?\n",
    );
    let added = jadpo(&root, &["migrate", "new", "add_flag"]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    assert!(
        latest(&root, "postgres.sql").contains("ADD COLUMN \"legacy_flag\""),
        "{}",
        latest(&root, "postgres.sql")
    );
}
