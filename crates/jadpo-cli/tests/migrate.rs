//! `jadpo migrate` (area 5, "Migrations"; area 14): `new` writes the next
//! migration from the entities, waits for the decisions a change needs and
//! finishes the migration once they are written; `jadpo check` agrees at
//! each step; `apply` and `status` run against a live database, which
//! `JADPO_SKIP_POSTGRES=1` skips, as does a field that allows less
//! replacing the stored values it no longer allows.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

const APP: &str = "application {}\n";
const TODO: &str = "domains/planning/entities/todo.jadpo";

use crate::support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("migrate-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// A project with one entity and no migration yet.
fn project(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", APP);
    write(
        &root,
        TODO,
        "entity Todo {\n    id: Uuid\n    title: Text\n}\n",
    );
    root
}

/// The migration folders, in order.
fn migrations(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join("database/migrations"))
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn the_first_migration_creates_the_tables_and_check_agrees() {
    let root = project("first");
    jadpo(&root, &["records", "update"]);
    let before = jadpo(&root, &["check"]);
    assert_eq!(before.code, 1);
    assert!(
        before.stdout.contains("rule: migration.out_of_date")
            && before.stdout.contains("Run `jadpo migrate new initial`"),
        "{}",
        before.stdout
    );
    let missing = jadpo(&root, &["migrate", "new"]);
    assert_eq!(missing.code, 2);
    assert!(
        missing.stderr.contains("rule: cli.migrate_arguments"),
        "{}",
        missing.stderr
    );
    let made = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    let names = migrations(&root);
    assert_eq!(names.len(), 1);
    assert!(
        names[0].ends_with("_initial") && names[0].len() == 21,
        "{names:?}"
    );
    assert!(
        made.stdout
            .contains("  compared the entities with an empty database\n"),
        "{}",
        made.stdout
    );
    assert!(made.stdout.contains("    + table  todo"), "{}", made.stdout);
    assert!(
        made.stdout
            .contains(&format!("Wrote database/migrations/{}/\n", names[0]))
    );
    assert!(
        made.stdout
            .contains("next: jadpo check  confirms the migration matches the entities")
    );
    let sql =
        fs::read_to_string(root.join(format!("database/migrations/{}/postgres.sql", names[0])))
            .expect("postgres.sql");
    assert!(sql.contains("-- store: main\nCREATE TABLE "), "{sql}");
    assert!(
        sql.contains(
            "CREATE TABLE \"todo\" (\n    \"id\" uuid NOT NULL,\n    \"title\" text NOT NULL,"
        ),
        "{sql}"
    );
    jadpo(&root, &["records", "update"]);
    assert_eq!(jadpo(&root, &["check"]).stdout, "checked: 2 files\n");
    let again = jadpo(&root, &["migrate", "new", "nothing"]);
    assert_eq!(again.code, 0);
    assert!(
        again
            .stdout
            .starts_with("Nothing to migrate: the entities match "),
        "{}",
        again.stdout
    );
    let status = jadpo(&root, &["migrate", "status"]);
    assert!(
        status.stdout.starts_with("1 migration\n"),
        "{}",
        status.stdout
    );
}

#[test]
fn a_change_that_needs_decisions_waits_for_them() {
    let root = project("decisions");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    write(
        &root,
        "domains/planning/types/rank.jadpo",
        "type Rank = Int {\n    min: 1\n    max: 5\n}\n",
    );
    write(
        &root,
        TODO,
        "use planning/types { Rank }\n\nentity Todo {\n    id: Uuid\n    rank: Rank\n}\n",
    );
    let started = jadpo(&root, &["migrate", "new", "add_rank"]);
    assert_eq!(started.code, 0, "{}", started.stdout);
    assert!(
        started
            .stdout
            .contains("    - title  text      retired: the column and its data stay"),
        "{}",
        started.stdout
    );
    assert!(
        started.stdout.contains("    + rank   smallint  new field"),
        "{}",
        started.stdout
    );
    assert!(
        started
            .stdout
            .contains("changes.jadpo  2 decisions to make"),
        "{}",
        started.stdout
    );
    assert!(
        started
            .stdout
            .contains("decision: existing `Todo` records need a `rank`"),
        "{}",
        started.stdout
    );
    assert!(
        started
            .stdout
            .contains("Set `backfill:` for `Todo.rank`, such as `1` (human)"),
        "{}",
        started.stdout
    );
    let latest = migrations(&root).pop().expect("a migration");
    let changes_path = format!("database/migrations/{latest}/changes.jadpo");
    jadpo(&root, &["records", "update"]);
    let check = jadpo(&root, &["check"]);
    assert_eq!(
        check
            .stdout
            .matches("rule: migration.decision_required")
            .count(),
        2,
        "{}",
        check.stdout
    );
    // A value its rules refuse doesn't fit.
    let changes = fs::read_to_string(root.join(&changes_path)).expect("changes");
    let written = changes
        .replace("    Todo.title: {}", "    Todo.title: FieldChange.retired")
        .replace(
            "    Todo.rank: {}",
            "    Todo.rank: {\n        backfill: 9\n    }",
        );
    write(&root, &changes_path, &written);
    jadpo(&root, &["records", "update"]);
    let check = jadpo(&root, &["check"]);
    assert!(
        check.stdout.contains("rule: migration.decision_invalid"),
        "{}",
        check.stdout
    );
    assert!(
        check.stdout.contains("it is above the field's `max`"),
        "{}",
        check.stdout
    );
    write(
        &root,
        &changes_path,
        &written.replace("backfill: 9", "backfill: 3"),
    );
    jadpo(&root, &["records", "update"]);
    let check = jadpo(&root, &["check"]);
    assert!(
        check.stdout.contains("rule: migration.unfinished"),
        "{}",
        check.stdout
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}", finished.stdout);
    assert!(
        finished
            .stdout
            .contains("changes.jadpo  every decision made"),
        "{}",
        finished.stdout
    );
    assert_eq!(migrations(&root).len(), 2, "finished in place");
    let sql = fs::read_to_string(root.join(format!("database/migrations/{latest}/postgres.sql")))
        .expect("sql");
    assert!(
        sql.contains("UPDATE \"todo\" SET \"rank\" = 3 WHERE \"rank\" IS NULL;"),
        "{sql}"
    );
    assert!(
        sql.contains("ALTER TABLE \"todo\" ALTER COLUMN \"title\" DROP NOT NULL;"),
        "{sql}"
    );
    jadpo(&root, &["records", "update"]);
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    assert_eq!(
        jadpo(&root, &["fmt", "--check"]).code,
        0,
        "changes.jadpo is formatted"
    );
}

#[test]
fn removing_an_entity_isnt_built_yet() {
    let root = project("remove");
    write(
        &root,
        "domains/planning/entities/tag.jadpo",
        "entity Tag {\n    id: Uuid\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    fs::remove_file(root.join(TODO)).expect("remove");
    let refused = jadpo(&root, &["migrate", "new", "remove_todo"]);
    assert_eq!(refused.code, 1);
    assert!(
        refused.stderr.contains("rule: toolchain.not_built_yet"),
        "{}",
        refused.stderr
    );
    assert!(
        refused.stderr.contains("entity `Todo` removed"),
        "{}",
        refused.stderr
    );
}

#[test]
fn apply_and_status_need_a_database() {
    let root = project("needs-url");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    let apply = jadpo(&root, &["migrate", "apply"]);
    assert_eq!(apply.code, 2);
    assert!(
        apply.stderr.contains("needs the database's URL"),
        "{}",
        apply.stderr
    );
    let wrong = jadpo(&root, &["migrate", "undo"]);
    assert_eq!(wrong.code, 2);
    assert!(
        wrong
            .stderr
            .contains("`undo` isn't something `jadpo migrate` does"),
        "{}",
        wrong.stderr
    );
}

// --- against a live database --------------------------------------------------

#[test]
fn apply_runs_pending_migrations_once_and_status_shows_them() {
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("migrate_apply").expect("database");
    let root = project("apply");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    let first = migrations(&root).remove(0);
    let pending = jadpo(&root, &["migrate", "status", "--url", &url]);
    assert_eq!(pending.code, 0, "{}{}", pending.stdout, pending.stderr);
    assert!(
        pending.stdout.contains("`main`: 0 applied, 1 pending"),
        "{}",
        pending.stdout
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert!(
        applied
            .stdout
            .starts_with("Applied 1 migration to `main`\n"),
        "{}",
        applied.stdout
    );
    assert!(applied.stdout.contains(&first), "{}", applied.stdout);
    let again = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert!(
        again.stdout.starts_with("`main` is up to date"),
        "{}",
        again.stdout
    );
    let status = jadpo(&root, &["migrate", "status", "--url", &url]);
    assert!(
        status.stdout.contains(&format!("    {first}  applied")),
        "{}",
        status.stdout
    );
    // A migration changed after it ran is refused.
    let sql = root.join(format!("database/migrations/{first}/postgres.sql"));
    let text = fs::read_to_string(&sql).expect("sql");
    fs::write(
        &sql,
        text.replace("\"title\" text NOT NULL", "\"title\" text"),
    )
    .expect("write");
    let changed = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(changed.code, 1);
    assert!(
        changed.stderr.contains("rule: migration.store_diverged"),
        "{}",
        changed.stderr
    );
}

/// Runs `sql` on the database at `url` and gives each row's text columns.
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
                        other => format!("{other:?}"),
                    })
                    .collect()
            })
            .collect()
    })
}

#[test]
fn allowing_less_replaces_the_stored_values_no_longer_allowed() {
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("migrate_narrowing")
        .expect("database");
    let root = folder("narrowing");
    write(&root, "app.jadpo", APP);
    let size = "domains/planning/types/size.jadpo";
    let label = "domains/planning/types/label.jadpo";
    write(
        &root,
        size,
        "type Size = Enum {\n    small\n    medium\n    large\n}\n",
    );
    write(&root, label, "type Label = Text {\n    max_length: 20\n}\n");
    write(
        &root,
        "domains/planning/entities/parcel.jadpo",
        "use planning/types { Label, Size }\n\nentity Parcel {\n    id: Uuid\n    size: Size\n    label: Label\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    assert_eq!(jadpo(&root, &["migrate", "apply", "--url", &url]).code, 0);
    rows(
        &url,
        "INSERT INTO \"parcel\" VALUES \
         ('00000000-0000-7000-8000-000000000001', 'large', 'a label of twenty ch'), \
         ('00000000-0000-7000-8000-000000000002', 'small', 'short') RETURNING \"id\"",
    );

    // A variant added allows more: a migration with nothing to run.
    write(
        &root,
        size,
        "type Size = Enum {\n    small\n    medium\n    large\n    huge\n}\n",
    );
    jadpo(&root, &["records", "update"]);
    let drift = jadpo(&root, &["check"]);
    assert!(
        drift.stdout.contains("migration.out_of_date"),
        "{}",
        drift.stdout
    );
    let grown = jadpo(&root, &["migrate", "new", "add_huge"]);
    assert!(
        grown
            .stdout
            .contains("no statements: nothing in the database changes"),
        "{}",
        grown.stdout
    );

    // A variant gone and a shorter label allow less: each needs a value
    // for what is stored.
    write(
        &root,
        size,
        "type Size = Enum {\n    small\n    medium\n    huge\n}\n",
    );
    write(&root, label, "type Label = Text {\n    max_length: 10\n}\n");
    let waiting = jadpo(&root, &["migrate", "new", "shrink"]);
    assert!(waiting.stdout.contains("allows less"), "{}", waiting.stdout);
    let shrink = migrations(&root).pop().expect("the migration");
    jadpo(&root, &["records", "update"]);
    let unfinished = jadpo(&root, &["check"]);
    assert!(
        unfinished.stdout.contains("migration.decision_required"),
        "{}",
        unfinished.stdout
    );
    write(
        &root,
        &format!("database/migrations/{shrink}/changes.jadpo"),
        "/// Stops large parcels and shortens labels.\nmigration {\n    Parcel.label: {\n        backfill: \"relabel\"\n    }\n    Parcel.size: {\n        backfill: Size.medium\n    }\n}\n",
    );
    let finished = jadpo(&root, &["migrate", "new", "shrink"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert_eq!(
        rows(
            &url,
            "SELECT \"size\", \"label\" FROM \"parcel\" ORDER BY \"id\""
        ),
        [vec!["medium", "relabel"], vec!["small", "short"]]
    );
}

#[test]
fn a_decision_that_doesnt_fit_says_why_and_stays_open() {
    let root = project("misfit");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    write(
        &root,
        "domains/planning/types/initials.jadpo",
        "type Initials = Text {\n    max_length: 3\n}\n",
    );
    write(
        &root,
        TODO,
        "use planning/types { Initials }\n\nentity Todo {\n    id: Uuid\n    title: Text\n    owner: Initials\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "add_owner"]).code, 0);
    let latest = migrations(&root).pop().expect("the migration");
    write(
        &root,
        &format!("database/migrations/{latest}/changes.jadpo"),
        "/// Adds an owner.\nmigration {\n    Todo.owner: {\n        backfill: \"nobody\"\n    }\n}\n",
    );
    let again = jadpo(&root, &["migrate", "new", "add_owner"]);
    assert!(
        again.stdout.contains("changes.jadpo  1 decision to make"),
        "{}",
        again.stdout
    );
    assert!(
        again.stdout.contains(
            "decision: this value can't fill `Todo.owner`: it is longer than the field's `max_length`"
        ),
        "{}",
        again.stdout
    );
}

#[test]
fn a_query_sorted_by_text_reads_its_index() {
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("migrate_sorted").expect("database");
    let root = folder("sorted");
    write(&root, "app.jadpo", APP);
    write(
        &root,
        "domains/planning/types/title.jadpo",
        "type Title = Text {\n    max_length: 80\n}\n",
    );
    write(
        &root,
        TODO,
        "use planning/types { Title }\n\nentity Todo {\n    id: Uuid\n    title: Title\n}\n",
    );
    write(
        &root,
        "domains/planning/queries/by_title.jadpo",
        "use planning/entities { Todo }\n\nquery by_title() -> List<Todo> {\n    from: Todo\n    order: [Todo.title asc]\n    limit: 10\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    let first = migrations(&root).remove(0);
    let sql = fs::read_to_string(root.join(format!("database/migrations/{first}/postgres.sql")))
        .expect("sql");
    assert!(sql.contains("(\"title\" COLLATE \"C\", \"id\")"), "{sql}");
    assert_eq!(jadpo(&root, &["migrate", "apply", "--url", &url]).code, 0);
    // The query's order, as its plan writes it, read from the index.
    use jadpo_runtime::{Database, SqlValue};
    let store = jadpo_host_native::postgres(&url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let plan = runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        transaction.batch("SET LOCAL enable_seqscan = off").await.expect("set");
        let rows = transaction
            .query(
                "EXPLAIN SELECT \"id\" FROM \"todo\" ORDER BY \"title\" COLLATE \"C\" ASC, \"id\" ASC LIMIT 11",
                &[],
            )
            .await
            .expect("explain");
        transaction.commit().await.expect("commit");
        rows.into_iter()
            .filter_map(|row| match row.into_iter().next() {
                Some(SqlValue::Text(line)) => Some(line),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    assert!(
        plan.contains("Scan using todo_title_id_sorted_idx"),
        "{plan}"
    );
    assert!(!plan.contains("Sort"), "{plan}");
}

/// A derived index records the first query, alphabetically, that needs
/// it, but only what defines it is compared (chunk 5.9d): a query that
/// sorts first needs no migration, and the next migration written for
/// another change records it as the reason with no step for the index.
#[test]
fn a_query_that_sorts_first_changes_no_index() {
    let root = folder("index-reason");
    write(&root, "app.jadpo", APP);
    write(
        &root,
        "domains/planning/types/title.jadpo",
        "type Title = Text {\n    max_length: 80\n}\n",
    );
    write(
        &root,
        TODO,
        "use planning/types { Title }\n\nentity Todo {\n    id: Uuid\n    title: Title\n}\n",
    );
    let query = |name: &str| {
        format!(
            "use planning/entities {{ Todo }}\n\nquery {name}() -> List<Todo> {{\n    from: Todo\n    order: [Todo.title asc]\n    limit: 10\n}}\n"
        )
    };
    write(
        &root,
        "domains/planning/queries/by_title.jadpo",
        &query("by_title"),
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    let snapshot = |name: &str| {
        fs::read_to_string(root.join(format!("database/migrations/{name}/snapshot.json")))
            .expect("snapshot.json")
    };
    let first = migrations(&root).remove(0);
    assert!(
        snapshot(&first).contains("\"reason\": \"query by_title\""),
        "{}",
        snapshot(&first)
    );
    // A query that sorts first, needing the same index.
    write(
        &root,
        "domains/planning/queries/a_by_title.jadpo",
        &query("a_by_title"),
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let again = jadpo(&root, &["migrate", "new", "more_titles"]);
    assert_eq!(again.code, 0, "{}{}", again.stdout, again.stderr);
    assert!(
        again.stdout.starts_with("Nothing to migrate: "),
        "{}",
        again.stdout
    );
    assert_eq!(migrations(&root).len(), 1);
    // Another change: its migration records the new reason, with no step
    // for the index.
    write(
        &root,
        TODO,
        "use planning/types { Title }\n\nentity Todo {\n    id: Uuid\n    title: Title\n    note: Text?\n}\n",
    );
    let made = jadpo(&root, &["migrate", "new", "add_note"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    let latest = migrations(&root).pop().expect("a migration");
    let sql = fs::read_to_string(root.join(format!("database/migrations/{latest}/postgres.sql")))
        .expect("postgres.sql");
    assert!(sql.contains("ADD COLUMN \"note\""), "{sql}");
    assert!(!sql.contains("INDEX"), "{sql}");
    assert!(!made.stdout.contains("index"), "{}", made.stdout);
    assert!(
        snapshot(&latest).contains("\"reason\": \"query a_by_title\""),
        "{}",
        snapshot(&latest)
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
}

/// A tenant's company and a project of it, which `shared` says every
/// company shares.
fn tenant_project(shared: bool) -> String {
    let tenant = if shared {
        "\n        tenant: Tenancy.shared"
    } else {
        ""
    };
    format!(
        r#"use planning/entities {{ Company }}

entity Project {{
    id: Uuid
    company_id: Company.id {{
        on_delete: OnDelete.cascade
    }}
    name: Text {{
        unique: true
    }}

    policy: {{
        read: [Access.authenticated]{tenant}
    }}
}}
"#
    )
}

/// A unique rule on a tenant's records holds within its tenant (area 9,
/// "A unique rule holds within its tenant"): a key that didn't hold the
/// tenancy is dropped and made again with its column, which needs no
/// decision, since a key with more columns refuses nothing the old one
/// allowed.
#[test]
fn a_unique_key_moved_within_the_tenant_is_made_again_without_a_decision() {
    let root = folder("tenant-key");
    write(
        &root,
        "app.jadpo",
        "use planning/entities { Person }\n\napplication {}\n\nprincipal {\n    user: {\n        subject: Person.handle\n    }\n}\n",
    );
    write(
        &root,
        "authentication.jadpo",
        "authentication {\n    api_token: {}\n}\n",
    );
    write(
        &root,
        "policy.jadpo",
        "use planning/entities { Company }\n\npolicy {\n    tenant: Company\n}\n",
    );
    write(
        &root,
        "domains/planning/entities/person.jadpo",
        "entity Person {\n    id: Uuid\n    handle: Text {\n        unique: true\n    }\n\n    policy: {\n        read: [id]\n        tenant: Tenancy.shared\n    }\n}\n",
    );
    write(
        &root,
        "domains/planning/types/role.jadpo",
        "type Role = Enum {\n    owner\n}\n",
    );
    write(
        &root,
        "domains/planning/entities/company.jadpo",
        "use planning/types { Role }\n\nentity Company {\n    id: Uuid\n\n    policy: {\n        read: [Role.owner]\n    }\n}\n",
    );
    write(
        &root,
        "domains/planning/entities/membership.jadpo",
        "use planning/entities { Company, Person }\nuse planning/types { Role }\n\nentity Membership {\n    id: Uuid\n    company_id: Company.id {\n        on_delete: OnDelete.cascade\n    }\n    person_id: Person.id {\n        on_delete: OnDelete.cascade\n    }\n    role: Role\n\n    membership: {\n        scope: company_id\n        member: person_id\n        role: role\n    }\n\n    policy: {\n        read: [person_id]\n    }\n}\n",
    );
    const PROJECT: &str = "domains/planning/entities/project.jadpo";
    write(&root, PROJECT, &tenant_project(true));
    let first = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(first.code, 0, "{}{}", first.stdout, first.stderr);
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    // The projects now belong to their company.
    write(&root, PROJECT, &tenant_project(false));
    jadpo(&root, &["records", "update"]);
    let before = jadpo(&root, &["check"]);
    assert!(
        before.stdout.contains("rule: migration.out_of_date"),
        "{}",
        before.stdout
    );
    let moved = jadpo(&root, &["migrate", "new", "names_within_company"]);
    assert_eq!(moved.code, 0, "{}{}", moved.stdout, moved.stderr);
    assert!(
        moved
            .stdout
            .contains("+ unique  (name, company_id)  for Project.name"),
        "{}",
        moved.stdout
    );
    let latest = migrations(&root).pop().expect("a migration");
    assert!(
        !root
            .join(format!("database/migrations/{latest}/changes.jadpo"))
            .exists(),
        "no decision to make"
    );
    let sql = fs::read_to_string(root.join(format!("database/migrations/{latest}/postgres.sql")))
        .expect("sql");
    assert!(
        sql.contains(
            "ALTER TABLE \"project\" DROP CONSTRAINT \"project_name_key\";\nALTER TABLE \"project\" ADD CONSTRAINT \"project_name_key\" UNIQUE (\"name\", \"company_id\");"
        ),
        "{sql}"
    );
    jadpo(&root, &["records", "update"]);
    let after = jadpo(&root, &["check"]);
    assert_eq!(after.code, 0, "{}", after.stdout);
}

/// Where a field is written in its entity is no part of the schema: a
/// field added in the middle is one new column, the fields after it don't
/// read as changed, and fields written in another order are nothing to
/// migrate.
#[test]
fn a_field_added_in_the_middle_changes_only_itself() {
    const BOOK: &str = "domains/lending/entities/book.jadpo";
    let root = folder("field-order");
    write(&root, "app.jadpo", APP);
    write(
        &root,
        BOOK,
        "entity Book {\n    id: Uuid\n    title: Text\n    added_at: Instant\n    changed_at: Instant\n    on_loan: Bool\n}\n",
    );
    let initial = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(initial.code, 0, "{}{}", initial.stdout, initial.stderr);
    write(
        &root,
        BOOK,
        "entity Book {\n    id: Uuid\n    title: Text\n    added_at: Instant\n    changed_at: Instant\n    author: Text?\n    on_loan: Bool\n}\n",
    );
    let added = jadpo(&root, &["migrate", "new", "add_author"]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    assert!(
        added.stdout.contains("+ author  text  new field"),
        "{}",
        added.stdout
    );
    for unchanged in ["added_at", "changed_at", "on_loan", "allows more"] {
        assert!(!added.stdout.contains(unchanged), "{}", added.stdout);
    }
    let latest = migrations(&root).pop().expect("a migration");
    let sql = fs::read_to_string(root.join(format!("database/migrations/{latest}/postgres.sql")))
        .expect("postgres.sql");
    assert!(
        sql.ends_with("-- store: main\nALTER TABLE \"book\" ADD COLUMN \"author\" text;\n"),
        "{sql}"
    );
    let snapshot =
        fs::read_to_string(root.join(format!("database/migrations/{latest}/snapshot.json")))
            .expect("snapshot.json");
    let schema = jadpo_graph::schema::Schema::read(&snapshot).expect("a snapshot");
    let order: Vec<&str> = schema.stores["main"].tables["book"]
        .ordered()
        .into_iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(
        order,
        ["id", "title", "added_at", "changed_at", "on_loan", "author"]
    );
    jadpo(&root, &["records", "update"]);
    let check = jadpo(&root, &["check"]);
    assert_eq!(check.code, 0, "{}", check.stdout);
    write(
        &root,
        BOOK,
        "entity Book {\n    on_loan: Bool\n    author: Text?\n    id: Uuid\n    changed_at: Instant\n    title: Text\n    added_at: Instant\n}\n",
    );
    jadpo(&root, &["records", "update"]);
    let check = jadpo(&root, &["check"]);
    assert_eq!(check.code, 0, "{}", check.stdout);
    let again = jadpo(&root, &["migrate", "new", "reorder"]);
    assert!(
        again
            .stdout
            .starts_with("Nothing to migrate: the entities match "),
        "{}",
        again.stdout
    );
}
