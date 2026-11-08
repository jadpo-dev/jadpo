//! What a project keeps if it stops using Jadpo (docs/leaving-jadpo.md):
//! its database is plain PostgreSQL that PostgreSQL's own `psql` creates
//! from the migrations and reads, with nothing of Jadpo's in between.
//!
//! The golden todo's committed migrations (`benchmarks/golden-todo/`) are
//! applied to an empty database by `psql` alone, and add tables and their
//! keys and indexes, and no function, trigger or extension; then a small
//! project's seed writes records through its actions and `psql` reads
//! them back as ordinary rows.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips
//! these tests. `psql` is found on `PATH`, through `pg_config`, or where
//! PostgreSQL is usually installed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn checkout() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the checkout")
        .to_path_buf()
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

/// PostgreSQL's `psql`: on `PATH`, else in the folder `pg_config` names,
/// else the newest of the usual installs.
fn psql() -> PathBuf {
    let on_path = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    let configured = Command::new("pg_config")
        .arg("--bindir")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()));
    let mut usual = Vec::new();
    for (parent, prefix, rest) in [
        ("/Applications/Postgres.app/Contents/Versions", "", "bin"),
        ("/usr/lib/postgresql", "", "bin"),
        ("/opt/homebrew/opt", "postgresql@", "bin"),
        ("/usr/local/opt", "postgresql@", "bin"),
    ] {
        let mut found: Vec<PathBuf> = fs::read_dir(parent)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
                    .map(|entry| entry.path().join(rest))
                    .collect()
            })
            .unwrap_or_default();
        found.sort();
        usual.extend(found.into_iter().rev());
    }
    on_path
        .into_iter()
        .chain(configured)
        .chain(usual)
        .map(|dir| dir.join("psql"))
        .find(|path| path.is_file())
        .expect("psql: these tests read the database with PostgreSQL's own client")
}

/// Runs `psql` against `address` with `args`, stopping at the first
/// error, and gives what it printed.
fn psql_run(address: &str, args: &[&str]) -> String {
    let output = Command::new(psql())
        .args(["-X", "-q", "-v", "ON_ERROR_STOP=1", "-d", address])
        .args(args)
        .output()
        .expect("psql runs");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "psql {args:?} failed:\n{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

/// Each row `sql` gives, its columns joined by `|`.
fn psql_rows(address: &str, sql: &str) -> Vec<String> {
    psql_run(address, &["-A", "-t", "-F", "|", "-c", sql])
        .lines()
        .map(str::to_string)
        .collect()
}

/// A new, empty database on the test server, as a `postgresql://` URL
/// both `psql` and a `PostgresUrl` setting take; `None` when live
/// PostgreSQL is skipped.
fn database(stem: &str) -> Option<String> {
    if !jadpo_test::live() {
        return None;
    }
    let server = jadpo_test::database::server().expect("a PostgreSQL server");
    let name = server.new_database(stem).expect("a database");
    Some(server.address(&name))
}

/// The functions, triggers and extensions a database has, so a test can
/// say the migrations added none.
fn machinery(address: &str) -> Vec<String> {
    psql_rows(
        address,
        "SELECT 'function ' || p.proname FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname NOT IN ('pg_catalog', 'information_schema') \
         UNION ALL SELECT 'trigger ' || tgname FROM pg_trigger WHERE NOT tgisinternal \
         UNION ALL SELECT 'extension ' || extname FROM pg_extension \
         ORDER BY 1",
    )
}

#[test]
fn the_golden_todos_migrations_are_plain_postgresql_psql_applies() {
    let Some(address) = database("leaving_golden_todo") else {
        return;
    };
    let before = machinery(&address);
    let migrations = checkout().join("benchmarks/golden-todo/database/migrations");
    let mut folders: Vec<PathBuf> = fs::read_dir(&migrations)
        .expect("the golden todo's migrations")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join("postgres.sql").is_file())
        .collect();
    folders.sort();
    assert!(folders.len() >= 2, "{folders:?}");
    for folder in &folders {
        let sql = folder.join("postgres.sql");
        psql_run(&address, &["-f", &sql.to_string_lossy()]);
    }
    // Every table is an entity's, named after it, or the runtime's own,
    // named `jadpo_`.
    let tables = psql_rows(
        &address,
        "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' ORDER BY 1",
    );
    let (runtime, entities): (Vec<&String>, Vec<&String>) = tables
        .iter()
        .partition(|table| table.starts_with("jadpo_"));
    assert_eq!(entities, ["service", "todo", "user"], "{tables:?}");
    assert!(runtime.iter().any(|table| *table == "jadpo_outbox"), "{tables:?}");
    // A column after each field, of PostgreSQL's own type for it.
    let columns = psql_rows(
        &address,
        "SELECT column_name || ' ' || data_type || ' ' || is_nullable FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'todo' ORDER BY ordinal_position",
    );
    for column in [
        "id uuid NO",
        "owner_id uuid NO",
        "title text NO",
        "status text NO",
        "due_at timestamp with time zone YES",
        "created_at timestamp with time zone NO",
    ] {
        assert!(columns.iter().any(|c| c == column), "{column}: {columns:?}");
    }
    // The database holds the keys, unique rules and references.
    let constraints = psql_rows(
        &address,
        "SELECT conrelid::regclass::text || ' ' || conname || ' ' || contype FROM pg_constraint \
         WHERE conrelid IN ('public.todo'::regclass, 'public.service'::regclass) ORDER BY 1",
    );
    assert_eq!(
        constraints,
        [
            "service service_name_key u",
            "service service_owner_id_fkey f",
            "service service_pkey p",
            "todo todo_owner_id_fkey f",
            "todo todo_pkey p",
        ],
        "{constraints:?}"
    );
    // Nothing of Jadpo's runs inside the database.
    assert_eq!(machinery(&address), before);
}

const CHORES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    database_url: PostgresUrl
}
"#,
    ),
    (
        "domains/house/types/priority.jadpo",
        r#"/// How soon a chore needs doing.
type Priority = Enum {
    low
    high
}
"#,
    ),
    (
        "domains/house/entities/chore.jadpo",
        r#"use house/types { Priority }

/// Something to do around the house.
entity Chore {
    id: Uuid
    title: Text
    done: Bool
    priority: Priority
}
"#,
    ),
    (
        "domains/house/actions/add_chore.jadpo",
        r#"use house/entities { Chore }
use house/types { Priority }

/// Adds a chore, not yet done.
action add_chore(title: Text, priority: Priority) -> Chore {
    return Chore.create {
        set: {
            title: title
            done: false
            priority: priority
        }
    }
}
"#,
    ),
    (
        "database/seeds/two_chores.jadpo",
        r#"use house
use house/types { Priority }

/// Two chores, for local development.
seed {
    house.add_chore("Water the plants", Priority.low)
    house.add_chore("Fix the gate", Priority.high)
}
"#,
    ),
];

#[test]
fn records_the_app_writes_are_rows_psql_reads() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("leaving-chores");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in CHORES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let Some(address) = database("leaving_chores") else {
        return;
    };
    write(&root, ".env.local", &format!("DATABASE_URL={address}\n"));
    let seeded = jadpo(&root, &["seed", "two_chores"]);
    assert_eq!(seeded.code, 0, "{}{}", seeded.stdout, seeded.stderr);
    assert_eq!(
        psql_rows(
            &address,
            "SELECT title, done, priority FROM chore ORDER BY title"
        ),
        ["Fix the gate|f|high", "Water the plants|f|low"]
    );
}
