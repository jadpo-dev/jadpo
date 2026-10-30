//! Upgrading past encoding 2 with a `Jwk` kept in an entity (area 19,
//! "Changing a standard type's encoding"; chunk A3). `fixtures/jwk-encoding-1/`
//! holds a project whose only migration Jadpo 0.1.0 wrote, at encoding 1,
//! before `Jwk` held `key_ops`: its `snapshot.json` and `postgres.sql` are
//! that release's output, unedited. After upgrading:
//!
//! - `jadpo check` refuses the project with `migration.out_of_date`, so the
//!   new column is never read before it exists;
//! - `jadpo migrate new` writes the one `text[]` column `key_ops` needs;
//! - a row written under 0.1.0's migration, before the column existed,
//!   reads back with `key_ops` as `none`.
//!
//! The project's own test, a key without `key_ops` kept and read back, runs
//! compiled beside the interpreter in the compiled corpus
//! (`compiled::the_corpus_passes_compiled_with_the_interpreters_records`).

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::{Executor, Outcome};
use jadpo_test::host::Harness;
use jadpo_value::Value;

use super::http::{jadpo, tempdir};

/// The fixture's folder.
pub(super) fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jwk-encoding-1")
}

/// Every file under `from`, by its path relative to `from`.
pub(super) fn files(from: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut folders = vec![from.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).expect("a folder").flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else {
                let relative = path
                    .strip_prefix(from)
                    .expect("inside")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((relative, fs::read_to_string(&path).expect("text")));
            }
        }
    }
    out.sort();
    out
}

fn project(dir: &tempdir::Dir) -> PathBuf {
    let root = dir.path().join("keys");
    for (path, text) in files(&fixture()) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
        fs::write(path, text).expect("written");
    }
    root
}

#[test]
fn a_project_from_encoding_1_is_told_to_migrate_and_writes_the_key_ops_column() {
    let dir = tempdir::Dir::new("jwk-upgrade-check");
    let root = project(&dir);
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check", "--json"]);
    assert_eq!(checked.code, 1, "{}{}", checked.stdout, checked.stderr);
    assert!(
        checked.stdout.contains("migration.out_of_date"),
        "{}",
        checked.stdout
    );
    let made = jadpo(&root, &["migrate", "new", "add_key_ops"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    let added = fs::read_dir(root.join("database/migrations"))
        .expect("the migrations")
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with("_add_key_ops")
        })
        .expect("the new migration");
    let sql = fs::read_to_string(added.path().join("postgres.sql")).expect("its SQL");
    let statements: Vec<&str> = sql
        .lines()
        .filter(|line| !line.starts_with("--") && !line.trim().is_empty())
        .collect();
    assert_eq!(
        statements,
        ["ALTER TABLE \"partner_key\" ADD COLUMN \"key_key_ops\" text[];"]
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
}

#[test]
fn a_key_kept_before_key_ops_existed_reads_back_with_none() {
    if !jadpo_test::live() {
        return;
    }
    let dir = tempdir::Dir::new("jwk-upgrade-rows");
    let root = project(&dir);
    let server = jadpo_test::database::server().expect("a PostgreSQL server");
    let url = server.create_database("jwk_upgrade").expect("a database");
    // 0.1.0's migration, and a row written as 0.1.0 wrote it.
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    let runtime = Runtime::new().expect("a runtime");
    let host = Harness::postgres(&url);
    runtime.block_on(host.rows(
        "INSERT INTO \"partner_key\" (\"id\", \"key_kty\", \"key_crv\", \"key_x\", \"key_kid\") VALUES ('00000000-0000-4000-8000-000000000001', 'OKP', 'Ed25519', '11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo', 'kept-in-0.1.0')",
    ));
    // The upgrade's migration.
    let made = jadpo(&root, &["migrate", "new", "add_key_ops"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    let rows = runtime.block_on(
        host.rows("SELECT \"key_kid\", (\"key_key_ops\" IS NULL)::text FROM \"partner_key\""),
    );
    assert_eq!(rows.len(), 1, "{rows:?}");
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let read = runtime.block_on(async {
        let id = program.plan().find("kept_keys").expect("the action");
        let mut ctx = host.ctx(&program);
        program.run(&mut ctx, id, Vec::new()).await
    });
    let Outcome::Value(Value::List(keys)) = read else {
        panic!("the keys: {read:?}");
    };
    let [Value::Record(kept)] = &keys[..] else {
        panic!("one key: {keys:?}");
    };
    let Some(Value::Record(key)) = kept.get("key") else {
        panic!("its key: {kept:?}");
    };
    assert_eq!(key.get("kid"), Some(&Value::text("kept-in-0.1.0")));
    assert_eq!(key.get("key_ops"), Some(&Value::None));
}
