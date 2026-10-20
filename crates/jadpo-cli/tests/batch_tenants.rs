//! Batches in an application with a tenant, against live PostgreSQL
//! (area 12, "A batch's tenant"; chunk 6.4c): a run works as the system
//! limited to the tenant its one saved value holding a tenant's id names,
//! so another tenant's records are never touched; a run that saves no
//! tenant id, or ids naming different tenants, reaches no tenant's
//! records.
//!
//! A household application: each sweep walks every chore the system may
//! see a page at a time and ticks it. Two households, Ash with three
//! chores and Birch with two. A sweep's `finished:` keeps its summary's
//! counts in a tally of the household it walked (chunk 6.4d).
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::runner::{Deferred, run_deferred};
use jadpo_runtime::{Database, Executor, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use home/entities { Person }

application {}

principal {
    user: {
        subject: Person.handle
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "home_session"
    }
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use home/entities { Household }

policy {
    tenant: Household
}
"#,
    ),
    (
        "domains/home/entities/chore.jadpo",
        r#"use home/entities { Household }
use home/types { HomeRole, Title }

entity Chore {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    title: Title {
        immutable: true
    }
    ticks: Int

    policy: {
        create: [HomeRole.member]
        read: [HomeRole.member, Access.system]
        update: [Access.system]
    }
}
"#,
    ),
    (
        "domains/home/entities/home_membership.jadpo",
        r#"use home/entities { Household, Person }
use home/types { HomeRole }

entity HomeMembership {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    person_id: Person.id {
        on_delete: OnDelete.cascade
    }
    role: HomeRole

    membership: {
        scope: household_id
        member: person_id
        role: role
    }

    policy: {
        create: [HomeRole.member]
        read: [person_id]
    }
}
"#,
    ),
    (
        "domains/home/entities/household.jadpo",
        r#"use home/types { HomeRole, Title }

entity Household {
    id: Uuid
    name: Title

    policy: {
        read: [HomeRole.member]
    }
}
"#,
    ),
    (
        "domains/home/entities/person.jadpo",
        r#"use home/types { Handle }

entity Person {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
        tenant: Tenancy.shared
    }
}
"#,
    ),
    (
        "domains/home/queries/all_chores.jadpo",
        r#"use home/entities { Chore }
use home/types { SweepPage }

query all_chores(page: SweepPage) -> Page<Chore> {
    from: Chore
    order: [Chore.title asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
"#,
    ),
    (
        "domains/home/actions/tick.jadpo",
        r#"use home/entities { Chore, ChoreNotFound }

action tick(chore: Chore) fails ChoreNotFound -> Unit {
    attempt Chore.update(chore.id) {
        set: {
            ticks: Chore.ticks + 1
        }
    }
}
"#,
    ),
    (
        "domains/home/actions/tick_with.jadpo",
        r#"use home/entities { Chore, ChoreNotFound, Household }

action tick_with(chore: Chore, also: Household.id) fails ChoreNotFound -> Unit {
    var ticked: Chore = attempt Chore.update(chore.id) {
        set: {
            ticks: Chore.ticks + 1
        }
    }
    assert ticked.household_id == also
}
"#,
    ),
    (
        "domains/home/actions/tick_once.jadpo",
        r#"use home/entities { Chore, ChoreNotFound }
use home/failures { ChoreTicked }

action tick_once(chore: Chore) fails ChoreTicked, ChoreNotFound -> Unit {
    if (chore.ticks > 0) {
        reject ChoreTicked {}
    }
    attempt Chore.update(chore.id) {
        set: {
            ticks: Chore.ticks + 1
        }
    }
}
"#,
    ),
    (
        "domains/home/actions/keep_tally.jadpo",
        r#"use home/entities { Household, HouseholdNotFound, Tally }

action keep_tally(summary: ImportSummary, household_id: Household.id) fails HouseholdNotFound -> Unit {
    var kept: Tally = attempt Tally.create {
        set: {
            household_id: household_id
            walked: summary.items
            skipped: summary.items_with_problems
        }
    }
    assert kept.walked == summary.items
}
"#,
    ),
    (
        // The run's tenant is the chore's household: `chore.household_id`
        // is declared `Household.id`, the tenant's `id` type.
        "domains/home/actions/sweep_and_tally.jadpo",
        r#"use home/actions { keep_tally, tick_once }
use home/entities { Chore }
use home/queries { all_chores }
use home/types { SweepPage }

action sweep_and_tally(chore: Chore, page: SweepPage) -> BatchRun {
    return batches.each(
        from: all_chores(page),
        each: found -> tick_once(found),
        finished: summary -> keep_tally(summary, chore.household_id),
        key: chore.id
    )
}
"#,
    ),
    (
        "domains/home/entities/tally.jadpo",
        r#"use home/entities { Household }
use home/types { HomeRole }

entity Tally {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    walked: Int
    skipped: Int

    policy: {
        create: [Access.system]
        read: [HomeRole.member, Access.system]
    }
}
"#,
    ),
    (
        "domains/home/failures/chore_ticked.jadpo",
        r#"type ChoreTicked = Failure {
    kind: FailureKind.rejected
    message: "This chore is ticked already."
}
"#,
    ),
    (
        "domains/home/actions/sweep_household.jadpo",
        r#"use home/actions { tick }
use home/entities { Household }
use home/queries { all_chores }
use home/types { SweepPage }

action sweep_household(household: Household, page: SweepPage) -> BatchRun {
    return batches.each(
        from: all_chores(page),
        each: chore -> tick(chore),
        key: household.id
    )
}
"#,
    ),
    (
        "domains/home/actions/sweep_unnamed.jadpo",
        r#"use home/actions { tick }
use home/queries { all_chores }
use home/types { SweepPage }

action sweep_unnamed(page: SweepPage) -> BatchRun {
    return batches.each(from: all_chores(page), each: chore -> tick(chore))
}
"#,
    ),
    (
        "domains/home/actions/sweep_pair.jadpo",
        r#"use home/actions { tick_with }
use home/entities { Household }
use home/queries { all_chores }
use home/types { SweepPage }

action sweep_pair(household: Household, also: Household, page: SweepPage) -> BatchRun {
    return batches.each(
        from: all_chores(page),
        each: chore -> tick_with(chore, also.id),
        key: household.id
    )
}
"#,
    ),
    (
        "domains/home/types/handle.jadpo",
        r#"type Handle = Text {
    min_length: 3
    max_length: 40
}
"#,
    ),
    (
        "domains/home/types/home_role.jadpo",
        r#"type HomeRole = Enum {
    member
}
"#,
    ),
    (
        "domains/home/types/sweep_page.jadpo",
        r#"type SweepPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}
"#,
    ),
    (
        "domains/home/types/title.jadpo",
        r#"type Title = Text {
    min_length: 1
    max_length: 80
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("batch-tenants-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

async fn execute(host: &Harness, sql: &str, params: &[SqlValue]) {
    host.store()
        .query_once(sql, params)
        .await
        .expect("the statement");
}

/// Each chore's title and how many times it was ticked, by title.
async fn ticks(host: &Harness) -> Vec<(String, i64)> {
    host.store()
        .query_once(
            "SELECT \"title\", \"ticks\"::int8 FROM \"chore\" ORDER BY \"title\"",
            &[],
        )
        .await
        .expect("the query")
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(title), SqlValue::Int8(n)] => (title.clone(), *n),
            other => panic!("a chore's row: {other:?}"),
        })
        .collect()
}

/// Each tally's household, items walked and items skipped.
async fn tallies(host: &Harness) -> Vec<(String, i64, i64)> {
    host.store()
        .query_once(
            "SELECT \"household_id\"::text, \"walked\"::int8, \"skipped\"::int8 FROM \"tally\" ORDER BY 1",
            &[],
        )
        .await
        .expect("the query")
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(household), SqlValue::Int8(walked), SqlValue::Int8(skipped)] => {
                (household.clone(), *walked, *skipped)
            }
            other => panic!("a tally's row: {other:?}"),
        })
        .collect()
}

/// The run's state and items done, as its row keeps them.
async fn kept(host: &Harness, run: &Value) -> (String, i64) {
    let id = match jadpo_runtime::to_sql(&field(run, "id"), jadpo_runtime::SqlType::Uuid) {
        Ok(id) => id,
        other => panic!("not an identity: {other:?}"),
    };
    let rows = host
        .store()
        .query_once(
            "SELECT \"state\", \"items_done\" FROM \"jadpo_batch_runs\" WHERE \"id\" = $1",
            &[id],
        )
        .await
        .expect("the query");
    match rows.first().map(Vec::as_slice) {
        Some([SqlValue::Text(state), SqlValue::Int8(done)]) => (state.clone(), *done),
        other => panic!("the run's row: {other:?}"),
    }
}

fn field(record: &Value, name: &str) -> Value {
    match record {
        Value::Record(record) => record.get(name).cloned().expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

fn page(size: i64) -> Value {
    Value::record(
        "SweepPage",
        vec![
            (Rc::from("page_size"), Value::Int(size)),
            (Rc::from("after"), Value::None),
        ],
    )
}

/// A household's record.
fn household(id: &str, name: &str) -> Value {
    Value::record(
        "Household",
        vec![
            (Rc::from("id"), Value::uuid(id)),
            (Rc::from("name"), Value::text(name)),
        ],
    )
}

const ASH: &str = "0a000000-0000-4000-8000-000000000001";
const BIRCH: &str = "0b000000-0000-4000-8000-000000000002";

/// Runs `action` with `args` as no one signed in, then the run it
/// started, to its end; gives the run.
async fn sweep(program: &Program, host: &Harness, action: &str, args: Vec<Value>) -> Value {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program.places());
    let outcome = program.run(&mut ctx, id, args).await;
    let work = ctx.take_deferred();
    let run = match outcome {
        Outcome::Value(run) => run,
        other => panic!("expected a run, got {other:?}"),
    };
    assert!(
        work.iter().any(|work| matches!(work, Deferred::Batch(_))),
        "the run is left for after the action"
    );
    let outcomes = run_deferred(program, host, work).await;
    assert!(matches!(&outcomes[..], [Outcome::Value(_)]), "{outcomes:?}");
    run
}

#[test]
fn a_run_reaches_only_the_tenant_its_one_saved_tenant_id_names() {
    let root = project("6.4c-runs");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if !jadpo_test::live() {
        return;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url).at(1_800_000_000_000).random_from(7);
    let program = &program;
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        execute(&host,
            "INSERT INTO \"household\" (\"id\", \"name\") VALUES ($1::uuid, 'Ash'), ($2::uuid, 'Birch')",
            &[SqlValue::Text(ASH.into()), SqlValue::Text(BIRCH.into())],
        )
        .await;
        for (n, (title, household)) in [
            ("ash-1", ASH),
            ("ash-2", ASH),
            ("ash-3", ASH),
            ("birch-1", BIRCH),
            ("birch-2", BIRCH),
        ]
        .into_iter()
        .enumerate()
        {
            execute(&host,
                "INSERT INTO \"chore\" (\"id\", \"household_id\", \"title\", \"ticks\") VALUES ($1::uuid, $2::uuid, $3, 0)",
                &[
                    SqlValue::Text(format!("0c000000-0000-4000-8000-00000000000{n}")),
                    SqlValue::Text(household.into()),
                    SqlValue::Text(title.into()),
                ],
            )
            .await;
        }
        // Each household as a record read through the caller's grants
        // would give it: a run names only a tenant its action reached
        // (chunk 6.6c).
        let ash = household(ASH, "Ash");
        let birch = household(BIRCH, "Birch");

        // One saved tenant id, its `key:`: the run walks Ash's chores in
        // pages of two and never sees Birch's.
        let run = sweep(
            program,
            &host,
            "sweep_household",
            vec![ash.clone(), page(2)],
        )
        .await;
        assert_eq!(kept(&host, &run).await, ("finished".to_string(), 3));
        assert_eq!(
            ticks(&host).await,
            [
                ("ash-1".to_string(), 1),
                ("ash-2".to_string(), 1),
                ("ash-3".to_string(), 1),
                ("birch-1".to_string(), 0),
                ("birch-2".to_string(), 0),
            ]
        );

        // No saved tenant id: the run reaches no tenant's records.
        let run = sweep(program, &host, "sweep_unnamed", vec![page(2)]).await;
        assert_eq!(kept(&host, &run).await, ("finished".to_string(), 0));

        // Two saved tenant ids naming different tenants: none either.
        let run = sweep(
            program,
            &host,
            "sweep_pair",
            vec![ash.clone(), birch.clone(), page(2)],
        )
        .await;
        assert_eq!(kept(&host, &run).await, ("finished".to_string(), 0));

        // Two naming the same tenant name that one: Birch's chores only.
        let run = sweep(
            program,
            &host,
            "sweep_pair",
            vec![birch.clone(), birch.clone(), page(2)],
        )
        .await;
        assert_eq!(kept(&host, &run).await, ("finished".to_string(), 2));
        assert_eq!(
            ticks(&host).await,
            [
                ("ash-1".to_string(), 1),
                ("ash-2".to_string(), 1),
                ("ash-3".to_string(), 1),
                ("birch-1".to_string(), 1),
                ("birch-2".to_string(), 1),
            ]
        );
    });
    drop(copy);
}

#[test]
fn a_runs_finished_keeps_its_counts_in_its_tenants_record() {
    // Chunk 6.4d: `finished:` stores what the summary counts in a tally of
    // the household the run is limited to, named by the chore it was
    // started from, and the run never reaches the other household.
    let root = project("6.4d-finished");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if !jadpo_test::live() {
        return;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url)
        .at(1_800_000_000_000)
        .random_from(11);
    let program = &program;
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        execute(&host,
            "INSERT INTO \"household\" (\"id\", \"name\") VALUES ($1::uuid, 'Ash'), ($2::uuid, 'Birch')",
            &[SqlValue::Text(ASH.into()), SqlValue::Text(BIRCH.into())],
        )
        .await;
        // Ash's third chore is ticked already, so its turn fails.
        for (n, (title, household, ticks)) in [
            ("ash-1", ASH, 0),
            ("ash-2", ASH, 0),
            ("ash-3", ASH, 5),
            ("birch-1", BIRCH, 0),
        ]
        .into_iter()
        .enumerate()
        {
            execute(&host,
                "INSERT INTO \"chore\" (\"id\", \"household_id\", \"title\", \"ticks\") VALUES ($1::uuid, $2::uuid, $3, $4)",
                &[
                    SqlValue::Text(format!("0c000000-0000-4000-8000-00000000000{n}")),
                    SqlValue::Text(household.into()),
                    SqlValue::Text(title.into()),
                    SqlValue::Int8(ticks),
                ],
            )
            .await;
        }
        let ash_chore = Value::record(
            "Chore",
            vec![
                (
                    Rc::from("id"),
                    Value::uuid("0c000000-0000-4000-8000-000000000000"),
                ),
                (Rc::from("household_id"), Value::uuid(ASH)),
                (Rc::from("title"), Value::text("ash-1")),
                (Rc::from("ticks"), Value::Int(0)),
            ],
        );
        let run = sweep(program, &host, "sweep_and_tally", vec![ash_chore, page(2)]).await;
        assert_eq!(kept(&host, &run).await, ("finished".to_string(), 3));
        assert_eq!(tallies(&host).await, [(ASH.to_string(), 3, 1)]);
        assert_eq!(
            ticks(&host).await,
            [
                ("ash-1".to_string(), 1),
                ("ash-2".to_string(), 1),
                ("ash-3".to_string(), 5),
                ("birch-1".to_string(), 0),
            ]
        );
    });
    drop(copy);
}
