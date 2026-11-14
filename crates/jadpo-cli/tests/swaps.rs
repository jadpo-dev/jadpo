//! A standard type swapped under stored data (area 19, "Swap to the
//! standard type"; area 18, "Acceptances"; chunk Q7): `jadpo fix <name>
//! swap` on a type already stored writes the edit, the records it changes
//! and the data migration, which converts each stored value by its shape,
//! and a type on the wire waits for a person's acceptance. The live tests
//! need PostgreSQL, which `JADPO_SKIP_POSTGRES=1` skips.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::Output;
use jadpo_runtime::{Database as _, SqlValue};

use crate::acceptances::{edit, git, jadpo, write};

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("swap-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn ok(out: &Output) {
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
}

const SLOT: &str = "domains/rooms/types/slot.jadpo";

/// A room booking app that stored its own slot before the check knew the
/// standard `InstantRange`: its first migration made `Booking`, with the
/// slot held as JSON, and then the decline that let it was taken away.
const ROOMS: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        SLOT,
        "/// When a room is booked, from its start to its end.\ndecline type.standard_exists: \"Kept until the swap\"\ntype Slot = Object {\n    starts_at: Instant\n    ends_at: Instant\n}\n",
    ),
    (
        "domains/rooms/entities/booking.jadpo",
        "use rooms/types { Slot }\n\n/// A room booked for a slot, and the slot it was held for first.\nentity Booking {\n    id: Uuid\n    slot: Slot\n    held: Slot?\n}\n",
    ),
];

fn rooms(name: &str) -> PathBuf {
    let root = folder(name);
    for (path, text) in ROOMS {
        write(&root, path, text);
    }
    ok(&jadpo(&root, &["migrate", "new", "initial"]));
    ok(&jadpo(&root, &["records", "update"]));
    ok(&jadpo(&root, &["check"]));
    edit(
        &root,
        SLOT,
        "decline type.standard_exists: \"Kept until the swap\"\n",
        "",
    );
    root
}

fn latest(root: &Path) -> String {
    let mut names: Vec<String> = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names.pop().expect("a migration")
}

fn file(root: &Path, name: &str, file: &str) -> String {
    fs::read_to_string(root.join(format!("database/migrations/{name}/{file}"))).expect(file)
}

#[test]
fn a_stored_type_swapped_writes_the_migration_that_converts_it() {
    let root = rooms("stored");
    let fixed = jadpo(&root, &["fix", "Slot", "swap"]);
    ok(&fixed);
    let name = latest(&root);
    assert!(name.ends_with("_swap_slot"), "{name}");
    assert_eq!(
        fixed.stdout,
        format!(
            "  domains/rooms/types/slot.jadpo
    - Slot           type, the standard InstantRange replaces it
    - slot.jadpo     file, nothing left in it

  domains/rooms/entities/booking.jadpo
    ~ line 6         InstantRange, was Slot
    ~ line 7         InstantRange, was Slot

  database/migrations/{name}/
    + snapshot.json  the whole schema at this point
    + postgres.sql   12 statements: add column, allow none, fill existing rows in batches in Jadpo, lock against writes, fill rows written meanwhile in Jadpo, add check, check every row has a value, require a value, drop constraint
    + changes.jadpo  no decision needed: each stored value is converted to its standard type

Swapped Slot for the standard InstantRange
"
        )
    );
    assert!(!root.join(SLOT).exists());
    // Nothing is left to decide: the migration converts both fields.
    let changes = file(&root, &name, "changes.jadpo");
    assert!(
        changes.contains(
            "    // `Booking.slot` was a `Slot`, swapped for the standard `InstantRange`:\n"
        ),
        "{changes}"
    );
    assert!(changes.contains("`Booking.held` was a `Slot`"), "{changes}");
    let sql = file(&root, &name, "postgres.sql");
    for field in ["slot", "held"] {
        assert!(
            sql.contains(&format!(
                "-- backfill Booking.{field}: in Jadpo, 1000 rows a batch"
            )),
            "{sql}"
        );
        assert!(
            sql.contains(&format!(
                "SELECT \"id\", \"{field}\" FROM \"booking\" WHERE \"id\" > $1 AND \"{field}__2\" IS NULL"
            )),
            "{sql}"
        );
    }
    ok(&jadpo(&root, &["check"]));
    let json = jadpo(&root, &["fix", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(document["data"]["findings"], serde_json::json!([]));
    if !jadpo_test::live() {
        return;
    }
    let url = first_applied(&root, "swap_stored");
    statement(
        &url,
        r#"INSERT INTO "booking" ("id", "slot", "held") SELECT gen_random_uuid(), jsonb_build_object('starts_at', '2026-10-26T09:00:00.000Z', 'ends_at', '2026-10-26T17:00:00.000Z'), CASE WHEN n % 2 = 0 THEN jsonb_build_object('starts_at', '2026-10-25T09:00:00.000Z', 'ends_at', '2026-10-25T12:30:00.000Z') END FROM generate_series(1, 1500) AS n"#,
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    ok(&applied);
    assert!(
        applied
            .stdout
            .contains("1500 rows filled in 2 batches in Jadpo"),
        "{}",
        applied.stdout
    );
    assert_eq!(
        rows(
            &url,
            r#"SELECT count(*) FROM "booking" WHERE "slot__2" = tstzrange('2026-10-26T09:00:00Z', '2026-10-26T17:00:00Z', '[)')"#
        ),
        [["1500"]]
    );
    assert_eq!(
        rows(
            &url,
            r#"SELECT count(*) FILTER (WHERE "held__2" = tstzrange('2026-10-25T09:00:00Z', '2026-10-25T12:30:00Z', '[)')), count(*) FILTER (WHERE "held__2" IS NULL) FROM "booking""#
        ),
        [["750", "750"]]
    );
}

#[test]
fn a_stored_value_that_isnt_the_standard_type_stops_the_migration() {
    let root = rooms("unconverted");
    ok(&jadpo(&root, &["fix", "Slot", "swap"]));
    if !jadpo_test::live() {
        return;
    }
    let url = first_applied(&root, "swap_unconverted");
    statement(
        &url,
        r#"INSERT INTO "booking" ("id", "slot") VALUES ('00000000-0000-4000-8000-000000000001', '{"starts_at": "2026-10-26T17:00:00.000Z", "ends_at": "2026-10-26T09:00:00.000Z"}')"#,
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 1, "{}{}", applied.stdout, applied.stderr);
    let said = format!("{}{}", applied.stdout, applied.stderr);
    assert!(said.contains("start_not_after_end"), "{said}");
    assert!(
        said.contains("00000000-0000-4000-8000-000000000001"),
        "{said}"
    );
    assert!(!said.contains("2026-10-26"), "{said}");
    assert_eq!(
        rows(
            &url,
            r#"SELECT count(*) FROM "booking" WHERE "slot__2" IS NULL"#
        ),
        [["1"]]
    );
}

#[test]
fn a_point_stored_as_its_own_object_is_converted_to_a_geography() {
    let root = folder("point");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/fleet/types/van_position.jadpo",
        "/// Where a van is.\ndecline type.standard_exists: \"Kept until the swap\"\ntype VanPosition = Object {\n    lat: Decimal\n    lng: Decimal\n}\n",
    );
    write(
        &root,
        "domains/fleet/entities/van.jadpo",
        "use fleet/types { VanPosition }\n\n/// A van and where it was last seen.\nentity Van {\n    id: Uuid\n    seen_at: VanPosition\n}\n",
    );
    ok(&jadpo(&root, &["migrate", "new", "initial"]));
    ok(&jadpo(&root, &["records", "update"]));
    edit(
        &root,
        "domains/fleet/types/van_position.jadpo",
        "decline type.standard_exists: \"Kept until the swap\"\n",
        "",
    );
    ok(&jadpo(&root, &["fix", "VanPosition", "swap"]));
    let name = latest(&root);
    assert!(
        file(&root, &name, "changes.jadpo")
            .contains("`Van.seen_at` was a `VanPosition`, swapped for the standard `Point`"),
    );
    ok(&jadpo(&root, &["check"]));
    if !jadpo_test::live() {
        return;
    }
    let url = first_applied(&root, "swap_point");
    statement(
        &url,
        r#"INSERT INTO "van" ("id", "seen_at") VALUES (gen_random_uuid(), '{"lat": "51.5072", "lng": "-0.1276"}')"#,
    );
    ok(&jadpo(&root, &["migrate", "apply", "--url", &url]));
    assert_eq!(
        rows(
            &url,
            r#"SELECT ST_Y("seen_at__2"::geometry)::text, ST_X("seen_at__2"::geometry)::text FROM "van""#
        ),
        [["51.5072", "-0.1276"]]
    );
}

/// The rooms, with a route that sends each booking's slot, committed on
/// `main`, then a branch `change`.
fn rooms_on_the_wire(name: &str) -> PathBuf {
    let root = rooms(name);
    write(
        &root,
        "domains/rooms/types/booking_view.jadpo",
        "use rooms/entities { Booking }\n\n/// A booking as anyone sees it.\ntype BookingView = Output {\n    pick: [Booking.id, Booking.slot]\n}\n",
    );
    write(
        &root,
        "domains/rooms/queries/all_bookings.jadpo",
        "use rooms/entities { Booking }\nuse rooms/types { BookingView }\n\n/// Every booking.\nquery all_bookings() -> List<BookingView> {\n    from: Booking\n    order: [Booking.id asc]\n    limit: 50\n}\n",
    );
    write(
        &root,
        "domains/rooms/routes/bookings.jadpo",
        "use rooms/queries { all_bookings }\nuse rooms/types { BookingView }\n\nroutes Bookings {\n    prefix: /bookings\n\n    GET / {\n        authentication: RouteAuthentication.none\n        output: List<BookingView>\n        run: all_bookings()\n    }\n}\n",
    );
    edit(
        &root,
        "domains/rooms/entities/booking.jadpo",
        "    held: Slot?\n",
        "    held: Slot?\n\n    policy: {\n        read: [Access.public]\n    }\n",
    );
    edit(
        &root,
        SLOT,
        "type Slot",
        "decline type.standard_exists: \"Kept until the swap\"\ntype Slot",
    );
    ok(&jadpo(&root, &["records", "update"]));
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "The rooms"]);
    ok(&jadpo(&root, &["check"]));
    git(&root, &["checkout", "-q", "-b", "change"]);
    edit(
        &root,
        SLOT,
        "decline type.standard_exists: \"Kept until the swap\"\n",
        "",
    );
    root
}

#[test]
fn a_type_on_the_wire_waits_for_a_persons_acceptance() {
    let root = rooms_on_the_wire("wire");
    let fixed = jadpo(&root, &["fix", "Slot", "swap"]);
    ok(&fixed);
    assert!(
        fixed.stdout.contains(
            "  api/main.jadpo_record\n    ~ main.jadpo_record  record, brought up to date\n"
        ),
        "{}",
        fixed.stdout
    );
    assert!(
        fixed.stdout.ends_with(
            "Swapped Slot for the standard InstantRange\n\nnext: jadpo acceptance new swap_slot  writes the draft a person gives the reason for\n"
        ),
        "{}",
        fixed.stdout
    );
    assert!(
        fs::read_to_string(root.join("api/main.jadpo_record"))
            .expect("the API record")
            .contains("    slot: InstantRange\n")
    );
    // The change clients see waits for a person; nothing else is left.
    let checked = jadpo(&root, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&checked.stdout).expect("JSON");
    let rules: Vec<&str> = document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter_map(|d| d["rule"].as_str())
        .collect();
    assert_eq!(rules, ["acceptance.missing"], "{}", checked.stdout);
    assert!(
        checked
            .stdout
            .contains("main.BookingView.slot: ApiChange.output_field_retyped"),
        "{}",
        checked.stdout
    );
}

#[test]
fn a_swap_where_a_migration_was_due_leaves_it_to_jadpo_migrate_new() {
    let root = rooms("pending");
    edit(
        &root,
        "domains/rooms/entities/booking.jadpo",
        "    held: Slot?\n",
        "    held: Slot?\n    note: Text?\n",
    );
    let before = latest(&root);
    let fixed = jadpo(&root, &["fix", "Slot", "swap"]);
    ok(&fixed);
    assert!(
        fixed
            .stdout
            .ends_with("next: jadpo check  shows the error the project still has\n"),
        "{}",
        fixed.stdout
    );
    // The migration the project needed takes the swap's conversion in.
    assert_eq!(latest(&root), before);
    ok(&jadpo(&root, &["migrate", "new", "note_and_slot"]));
    let name = latest(&root);
    assert!(
        file(&root, &name, "changes.jadpo").contains("`Booking.slot` was a `Slot`"),
        "{}",
        file(&root, &name, "changes.jadpo")
    );
    ok(&jadpo(&root, &["check"]));
}

/// Applies the first migration only, to a database of its own.
fn first_applied(root: &Path, database: &str) -> String {
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database(database).expect("database");
    let migrations = steps(root);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let store = jadpo_host_native::postgres(&url).expect("store");
    runtime
        .block_on(jadpo_runtime::migrate(&store, &migrations[..1]))
        .expect("the first migration");
    url
}

/// The store's migrations, as `migrate apply` hands them to the runtime.
fn steps(root: &Path) -> Vec<jadpo_runtime::Migration> {
    let mut names: Vec<String> = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
        .into_iter()
        .flat_map(|name| {
            let sql = file(root, &name, "postgres.sql");
            jadpo_graph::migration::sections(&sql)
                .into_iter()
                .map(move |section| jadpo_runtime::Migration {
                    name: name.clone(),
                    digest: jadpo_graph::migration::section_digest(&section.sql),
                    sql: section.sql,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn statement(url: &str, sql: &str) {
    let store = jadpo_host_native::postgres(url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        transaction.batch(sql).await.expect("statement");
        transaction.commit().await.expect("commit");
    });
}

fn rows(url: &str, sql: &str) -> Vec<Vec<String>> {
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
                        SqlValue::Int8(n) => n.to_string(),
                        SqlValue::Null(_) => "NULL".to_string(),
                        other => format!("{other:?}"),
                    })
                    .collect()
            })
            .collect()
    })
}
