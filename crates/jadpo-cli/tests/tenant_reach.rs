//! The tenant background work names, against live PostgreSQL (area 9,
//! "Background work"; area 12, "Background work and tenants"): an event's
//! subscribers and a batch's run work as the system within the tenant a
//! value names, so that value comes from a record of the tenant the action
//! read under its caller's policy, and a caller who isn't a member there
//! reaches nothing; and a batch's `key:` is unique per call, tenant and
//! key, so two households using the same key get runs of their own.
//!
//! A household application: Ann belongs to Ash, Bob to Birch. `nudge`
//! reads the household it is given and emits `Nudged`, whose subscriber
//! keeps a tally in that household; `sweep` reads it and starts a run
//! ticking its chores, keyed by the round's name.
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
use jadpo_runtime::{Caller, Executor, Outcome, SqlValue};
use jadpo_std::http::PrincipalKind;
use jadpo_term::Environment;
use jadpo_test::host::Bench;
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

application {
    events: {
        store: main
    }
}

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
        r#"use home
use home/entities { Household }

policy {
    tenant: Household
    actions: {
        home.found: [Access.authenticated]
    }
}
"#,
    ),
    (
        "domains/home/actions/join.jadpo",
        r#"use home/entities { Person }
use home/failures { HandleTaken }
use home/types { Handle }

action join(handle: Handle) fails HandleTaken -> Person {
    return attempt Person.create {
        set: {
            handle: handle
        }
        conflict: {
            Person.handle: HandleTaken
        }
    }
}
"#,
    ),
    (
        "domains/home/failures/handle_taken.jadpo",
        r#"type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
"#,
    ),
    (
        "domains/home/actions/found.jadpo",
        r#"use home/entities { HomeMembership, Household }
use home/types { HomeRole, Title }

action found(name: Title, by: Principal.user) -> Household {
    var household: Household = Household.create {
        set: {
            name: name
        }
    }
    var held: HomeMembership = HomeMembership.create {
        set: {
            household_id: household.id
            person_id: by.id
            role: HomeRole.member
        }
    }
    assert held.role == HomeRole.member
    return household
}
"#,
    ),
    (
        "domains/home/actions/add_chore.jadpo",
        r#"use home/entities { Chore, Household, HouseholdNotFound }
use home/types { Title }

action add_chore(household_id: Household.id, title: Title) fails HouseholdNotFound -> Chore {
    return attempt Chore.create {
        set: {
            household_id: household_id
            title: title
            ticks: 0
        }
    }
}
"#,
    ),
    (
        // The allowed way: the household read through the caller's
        // membership, then its own id in the event.
        "domains/home/actions/nudge.jadpo",
        r#"use home/entities { Household, HouseholdNotFound }
use home/events { Nudged }

action nudge(household_id: Household.id) fails HouseholdNotFound -> Unit {
    var household: Household = attempt Household.get(household_id)
    emit Nudged {
        household_id: household.id
    }
}
"#,
    ),
    (
        "domains/home/actions/keep_tally.jadpo",
        r#"use home/entities { Household, HouseholdNotFound, Tally }

action keep_tally(household_id: Household.id) fails HouseholdNotFound -> Unit {
    var kept: Tally = attempt Tally.create {
        set: {
            household_id: household_id
        }
    }
    assert kept.household_id == household_id
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
        // The run's household is the one read here, saved as `from:`'s
        // argument; its key is the round's name, which two households may
        // share.
        "domains/home/actions/sweep.jadpo",
        r#"use home/actions { tick }
use home/entities { Household, HouseholdNotFound }
use home/queries { house_chores }
use home/types { RoundName, SweepPage }

action sweep(household_id: Household.id, round: RoundName, page: SweepPage) fails HouseholdNotFound -> BatchRun {
    var household: Household = attempt Household.get(household_id)
    return batches.each(from: house_chores(household.id, page), each: chore -> tick(chore), key: round)
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
    title: Title
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
        "domains/home/entities/tally.jadpo",
        r#"use home/entities { Household }
use home/types { HomeRole }

entity Tally {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        create: [Access.system]
        read: [HomeRole.member]
    }
}
"#,
    ),
    (
        "domains/home/events/nudged.jadpo",
        r#"use home/entities { Household }

/// Someone nudged a household.
type Nudged = Event {
    household_id: Household.id
}
"#,
    ),
    (
        "domains/home/queries/house_chores.jadpo",
        r#"use home/entities { Chore, Household }
use home/types { SweepPage }

query house_chores(household_id: Household.id, page: SweepPage) -> Page<Chore> {
    from: Chore
    where: Chore.household_id == household_id
    order: [Chore.title asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
"#,
    ),
    (
        "domains/home/subscribers/tally_nudge.jadpo",
        r#"use home/actions { keep_tally }
use home/events { Nudged }

/// Keeps a tally in the household nudged.
subscriber tally_nudge {
    on: Nudged
    run: keep_tally(event.household_id)
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
        "domains/home/types/round_name.jadpo",
        r#"type RoundName = Text {
    min_length: 1
    max_length: 40
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
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("tenant-reach-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

/// Each household's name and how many tallies it holds, by name.
async fn tallies(host: &Bench) -> Vec<(String, i64)> {
    host.rows("SELECT \"h\".\"name\", count(\"t\".\"id\") FROM \"household\" AS \"h\" LEFT JOIN \"tally\" AS \"t\" ON \"t\".\"household_id\" = \"h\".\"id\" GROUP BY \"h\".\"name\" ORDER BY \"h\".\"name\"")
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(name), SqlValue::Int8(n)] => (name.clone(), *n),
            other => panic!("not a tally: {other:?}"),
        })
        .collect()
}

/// Each chore's title and ticks, by title.
async fn ticks(host: &Bench) -> Vec<(String, i64)> {
    host.rows("SELECT \"title\", \"ticks\"::int8 FROM \"chore\" ORDER BY \"title\"")
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(title), SqlValue::Int8(n)] => (title.clone(), *n),
            other => panic!("a chore's row: {other:?}"),
        })
        .collect()
}

/// Runs `action` with `args` as `caller`, signed in, or as no one, and
/// gives back its outcome and the work it left for after its response.
async fn call(
    program: &Program,
    host: &Bench,
    action: &str,
    args: Vec<Value>,
    caller: Option<&Value>,
) -> (Outcome, Vec<Deferred>) {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program.places());
    ctx.set_caller(caller.map(|person| Caller {
        value: person.clone(),
        key: "user".to_string(),
        session: None,
    }));
    let outcome = program.run(&mut ctx, id, args).await;
    (outcome, ctx.take_deferred())
}

fn value(outcome: Outcome) -> Value {
    match outcome {
        Outcome::Value(value) => value,
        other => panic!("expected a value, got {other:?}"),
    }
}

fn field(record: &Value, name: &str) -> Value {
    match record {
        Value::Record(record) => record.get(name).cloned().expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

fn signed_in(person: &Value) -> Value {
    jadpo_runtime::http::identity::principal(
        PrincipalKind::User,
        field(person, "id"),
        field(person, "handle"),
    )
}

fn page() -> Value {
    Value::record(
        "SweepPage",
        vec![
            (Rc::from("page_size"), Value::Int(25)),
            (Rc::from("after"), Value::None),
        ],
    )
}

/// When the test runs: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;

#[test]
fn a_caller_reaches_no_other_tenant_through_an_event_or_a_batch() {
    let root = project("6.6c");
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
    let host = Bench::postgres(&url).at(T0).random_from(0);
    let program = &program;
    let host = &host;
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let ann = value(
            call(program, host, "join", vec![Value::text("ann")], None)
                .await
                .0,
        );
        let bob = value(
            call(program, host, "join", vec![Value::text("bob")], None)
                .await
                .0,
        );
        let (ann_in, bob_in) = (signed_in(&ann), signed_in(&bob));
        let ash = value(
            call(
                program,
                host,
                "found",
                vec![Value::text("Ash"), ann_in.clone()],
                Some(&ann_in),
            )
            .await
            .0,
        );
        let birch = value(
            call(
                program,
                host,
                "found",
                vec![Value::text("Birch"), bob_in.clone()],
                Some(&bob_in),
            )
            .await
            .0,
        );
        for (household, title, by) in [
            (&ash, "ash-dishes", &ann_in),
            (&ash, "ash-floor", &ann_in),
            (&birch, "birch-bins", &bob_in),
        ] {
            let (added, _) = call(
                program,
                host,
                "add_chore",
                vec![field(household, "id"), Value::text(title)],
                Some(by),
            )
            .await;
            value(added);
        }

        // Bob nudges Ash, a household he doesn't belong to: he finds no
        // household, so the action fails, no event is kept and the system
        // writes nothing in Ash.
        let (refused, nothing) = call(
            program,
            host,
            "nudge",
            vec![field(&ash, "id")],
            Some(&bob_in),
        )
        .await;
        assert!(matches!(refused, Outcome::Failed(_)), "{refused:?}");
        assert!(nothing.is_empty(), "{nothing:?}");
        assert_eq!(
            host.count("SELECT count(*) FROM \"jadpo_outbox\"").await,
            0,
            "no event kept"
        );
        // His own household's nudge is delivered there.
        let (nudged, work) = call(
            program,
            host,
            "nudge",
            vec![field(&birch, "id")],
            Some(&bob_in),
        )
        .await;
        value(nudged);
        let ran = run_deferred(program, host, work).await;
        assert!(matches!(&ran[..], [Outcome::Value(_)]), "{ran:?}");
        assert_eq!(
            tallies(host).await,
            [("Ash".to_string(), 0), ("Birch".to_string(), 1)]
        );

        // Bob sweeps Ash: no household again, so no run starts and Ash's
        // chores are untouched.
        let (refused, nothing) = call(
            program,
            host,
            "sweep",
            vec![field(&ash, "id"), Value::text("spring"), page()],
            Some(&bob_in),
        )
        .await;
        assert!(matches!(refused, Outcome::Failed(_)), "{refused:?}");
        assert!(nothing.is_empty(), "{nothing:?}");
        assert_eq!(
            host.count("SELECT count(*) FROM \"jadpo_batch_runs\"")
                .await,
            0,
            "no run started"
        );

        // Ann and Bob each sweep their own household with the same key:
        // two runs, each in its own household. Before chunk 6.6c, Bob's
        // start found Ann's run and ticked nothing of Birch's.
        let mut runs = Vec::new();
        for (household, by) in [(&ash, &ann_in), (&birch, &bob_in), (&ash, &ann_in)] {
            let (started, work) = call(
                program,
                host,
                "sweep",
                vec![field(household, "id"), Value::text("spring"), page()],
                Some(by),
            )
            .await;
            runs.push(field(&value(started), "id"));
            run_deferred(program, host, work).await;
        }
        assert_ne!(runs[0], runs[1], "a run of each household");
        assert_eq!(runs[0], runs[2], "Ann's second start finds her run");
        assert_eq!(
            host.count("SELECT count(*) FROM \"jadpo_batch_runs\"")
                .await,
            2
        );
        assert_eq!(
            ticks(host).await,
            [
                ("ash-dishes".to_string(), 1),
                ("ash-floor".to_string(), 1),
                ("birch-bins".to_string(), 1),
            ]
        );
    });
    drop(copy);
}
