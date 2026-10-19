//! Every statement the runtime runs on its system tables, prepared on live
//! PostgreSQL against the tables a migration makes of them (area 5,
//! "System tables"; chunk 6.8). The tables are defined once, in
//! `jadpo_std::system`; the checker's schema of them is what a project's
//! migrations create, and `jadpo_runtime::statements` lists every
//! statement the runtime runs on them, so a column one names that the
//! table doesn't have, or a type it doesn't take, fails here rather than
//! in a deployment.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::collections::BTreeMap;

use jadpo_graph::schema::{Schema, Store};
use jadpo_host_native::{Runtime, postgres};
use jadpo_runtime::Database;

/// Every system table in one store, migrated as `jadpo migrate new` would.
fn steps() -> BTreeMap<String, Vec<jadpo_runtime::Migration>> {
    let tables = jadpo_std::system::ALL
        .into_iter()
        .map(|system| {
            (
                system.table.to_string(),
                jadpo_check::data::system_table(system),
            )
        })
        .collect();
    let mut schema = Schema::default();
    schema.stores.insert(
        "main".to_string(),
        Store {
            driver: "postgres".to_string(),
            tables,
        },
    );
    let name = "20261020T0000_system_tables";
    let sql = jadpo_graph::migration::migration_sql(&Schema::default(), &schema, &|_| None)
        .expect("the system tables migrate")
        .to_document(name);
    let mut steps: BTreeMap<String, Vec<jadpo_runtime::Migration>> = BTreeMap::new();
    for section in jadpo_graph::migration::sections(&sql) {
        steps
            .entry(section.store)
            .or_default()
            .push(jadpo_runtime::Migration {
                name: name.to_string(),
                digest: jadpo_graph::migration::section_digest(&section.sql),
                sql: section.sql,
            });
    }
    steps
}

#[test]
fn every_runtime_statement_prepares_against_the_migrated_system_tables() {
    let statements = jadpo_runtime::statements::system_statements();
    assert!(statements.len() > 50, "{}", statements.len());
    if !jadpo_test::live() {
        return;
    }
    let databases = jadpo_test::database::TestDatabases::new(steps(), false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let store = postgres(&url).expect("store");
    let runtime = Runtime::new().expect("runtime");
    let refused: Vec<String> = runtime.block_on(async {
        let mut refused = Vec::new();
        for (at, (name, sql)) in statements.iter().enumerate() {
            // Prepared, never run: PostgreSQL checks every table, column
            // and type it names, and the parameters' types it works out.
            let mut transaction = store.begin().await.expect("a transaction");
            let prepared = transaction
                .batch(&format!(
                    "PREPARE \"jadpo_6_8_{at}\" AS {sql}; DEALLOCATE \"jadpo_6_8_{at}\""
                ))
                .await;
            let _ = transaction.rollback().await;
            if let Err(error) = prepared {
                refused.push(format!("{name}: {error:?}"));
            }
        }
        refused
    });
    assert!(refused.is_empty(), "{}", refused.join("\n"));
}
