//! Schedules against live PostgreSQL (area 12, "Schedules" and "Watching
//! schedules"; chunk 6.2): the tick runs a schedule when it is due, under
//! a lease, so two instances ticking at once never both run it; a due
//! tick that finds it leased or paused is skipped and recorded; ticks
//! missed while the application was down become one run on the same
//! grid; a run is the system with no tenant, reaching every household;
//! and the tick sweeps the runtime's own tables: a delivery left behind
//! is delivered, expired idempotency keys and events every subscriber has
//! had are deleted, and old run records go after 30 days.
//!
//! A household application: `chore_census` takes a census of every chore each
//! 15 minutes, emitting `CensusTaken`, which `ring_bell` rings a bell for,
//! failing until one is hung.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_interp::Program;
use jadpo_runtime::schedules::{Ran, Skipped, tick};
use jadpo_runtime::{Caller, Clock, Ctx, Database, Env, Executor, Host, Outcome, Row, SqlValue};
use jadpo_std::http::PrincipalKind;
use jadpo_term::Environment;
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
        "domains/home/actions/add_chore.jadpo",
        r#"use home/entities { Chore, Household, HouseholdNotFound, Person, PersonNotFound }
use home/types { ChoreView, Title }

action add_chore(household_id: Household.id, owner_id: Person.id, title: Title) fails HouseholdNotFound, PersonNotFound -> ChoreView {
    var chore: Chore = attempt Chore.create {
        set: {
            household_id: household_id
            owner_id: owner_id
            title: title
        }
    }
    return ChoreView {
        id: chore.id
        title: chore.title
    }
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
        "domains/home/actions/hang_bell.jadpo",
        r#"use home/entities { Bell }
use home/types { BellView }

action hang_bell() -> BellView {
    var bell: Bell = Bell.create {
        set: {}
    }
    return BellView {
        id: bell.id
    }
}
"#,
    ),
    (
        "domains/home/actions/join.jadpo",
        r#"use home/entities { Person }
use home/types { Handle }
use home/failures { HandleTaken }

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
        "domains/home/actions/ring.jadpo",
        r#"use home/failures { BellMissing }
use home/queries { bells }
use home/types { BellView }

action ring() fails BellMissing -> Unit {
    var hung: List<BellView> = bells()
    if (list.count(items: hung) == 0) {
        reject BellMissing {}
    }
}
"#,
    ),
    (
        "domains/home/actions/take_census.jadpo",
        r#"use home/entities { Census }
use home/events { CensusTaken }
use home/queries { chores }
use home/types { ChoreView, Count, TakenAt }

action take_census() -> Unit {
    var seen: List<ChoreView> = chores()
    var census: Census = Census.create {
        set: {
            seen: Count(list.count(items: seen))
            taken_at: TakenAt(clock.now)
        }
    }
    emit CensusTaken {
        census_id: census.id
    }
}
"#,
    ),
    (
        "domains/home/entities/bell.jadpo",
        r#"entity Bell {
    id: Uuid

    policy: {
        create: [Access.authenticated]
        read: [Access.system]
        tenant: Tenancy.shared
    }
}
"#,
    ),
    (
        "domains/home/entities/census.jadpo",
        r#"use home/types { Count, TakenAt }

entity Census {
    id: Uuid
    seen: Count
    taken_at: TakenAt

    policy: {
        create: [Access.system]
        read: [Access.system]
        tenant: Tenancy.shared
    }
}
"#,
    ),
    (
        "domains/home/entities/chore.jadpo",
        r#"use home/entities { Household, Person }
use home/types { HomeRole, Title }

entity Chore {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    owner_id: Person.id {
        on_delete: OnDelete.restricted
    }
    title: Title

    policy: {
        create: [HomeRole.member]
        read: [owner_id, Access.system]
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
        "domains/home/events/census_taken.jadpo",
        r#"use home/entities { Census }

/// A census of the chores was taken.
type CensusTaken = Event {
    census_id: Census.id
}
"#,
    ),
    (
        "domains/home/failures/bell_missing.jadpo",
        r#"type BellMissing = Failure {
    kind: FailureKind.unavailable
    message: "No bell is hung yet."
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
        "domains/home/queries/bells.jadpo",
        r#"use home/entities { Bell }
use home/types { BellView }

query bells() -> List<BellView> {
    from: Bell
    order: [Bell.id asc]
    limit: 1
}
"#,
    ),
    (
        "domains/home/queries/chores.jadpo",
        r#"use home/entities { Chore }
use home/types { ChoreView }

query chores() -> List<ChoreView> {
    from: Chore
    order: [Chore.title asc]
    limit: 50
}
"#,
    ),
    (
        "domains/home/schedules/chore_census.jadpo",
        r#"use home/actions { take_census }

/// Counts every household's chores.
schedule chore_census {
    every: 15m
    run: take_census()
}
"#,
    ),
    (
        "domains/home/subscribers/ring_bell.jadpo",
        r#"use home/actions { ring }
use home/events { CensusTaken }

/// Rings the bell for each census, once one is hung.
subscriber ring_bell {
    on: CensusTaken
    run: ring()
    retry: {
        attempts: 1
        within: 1h
    }
}
"#,
    ),
    (
        "domains/home/types/bell_view.jadpo",
        r#"use home/entities { Bell }

type BellView = Output {
    pick: [Bell.id]
}
"#,
    ),
    (
        "domains/home/types/chore_view.jadpo",
        r#"use home/entities { Chore }

type ChoreView = Output {
    pick: [Chore.id, Chore.title]
}
"#,
    ),
    (
        "domains/home/types/count.jadpo",
        r#"type Count = Int {
    min: 0
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
        "domains/home/types/taken_at.jadpo",
        r#"/// When a census was taken.
type TakenAt = Instant {}
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
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("schedules-6.2-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    // The event record, `domains/home/events/events.jadpo_record` (chunk
    // 6.1c).
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

/// A store, a clock the test moves, and random bytes from a counter.
struct Store {
    db: Postgres,
    seeds: Cell<u64>,
    clock: Cell<i64>,
    drawn: Cell<u64>,
}

impl Host for Store {
    fn database(&self, _store: &str) -> Option<&dyn Database> {
        Some(&self.db)
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        for chunk in bytes.chunks_mut(8) {
            self.drawn.set(self.drawn.get() + 1);
            let mut z = self.drawn.get().wrapping_mul(0x9E37_79B9_7F4A_7C15);
            z = (z ^ (z >> 31)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            for (to, from) in chunk.iter_mut().zip(z.to_be_bytes()) {
                *to = from;
            }
        }
        true
    }

    fn now_ms(&self) -> Option<i64> {
        Some(self.clock.get())
    }
}

impl Store {
    fn new(url: &str, offset: u64) -> Store {
        Store {
            db: postgres(url).expect("store"),
            seeds: Cell::new(0),
            clock: Cell::new(T0),
            drawn: Cell::new(offset),
        }
    }

    fn ctx<'p>(&'p self, program: &'p Program) -> Ctx<'p> {
        self.seeds.set(self.seeds.get() + 1);
        let env = Env {
            seed: Some(self.seeds.get()),
            clock: Clock::fixed(self.clock.get()),
        };
        Ctx::new(program.places(), self, env)
    }

    async fn rows(&self, sql: &str) -> Vec<Row> {
        self.db
            .query_once(sql, &[])
            .await
            .expect("the store answers")
    }

    async fn count(&self, sql: &str) -> i64 {
        match self.rows(sql).await.first().and_then(|row| row.first()) {
            Some(SqlValue::Int8(n)) => *n,
            other => panic!("not a count: {other:?}"),
        }
    }

    /// Each run record's outcome, the reason a tick skipped it, its due
    /// time as minutes after `T0` and its events, oldest first.
    async fn runs(&self) -> Vec<(String, Option<String>, i64, Option<i32>)> {
        self.rows("SELECT \"outcome\", \"reason\", ((extract(epoch FROM \"due_slot\") * 1000 - 1792400400000) / 60000)::int8, \"events\" FROM \"jadpo_schedule_runs\" ORDER BY \"started_at\", \"due_slot\"")
            .await
            .into_iter()
            .map(|row| match row.as_slice() {
                [SqlValue::Text(outcome), reason, SqlValue::Int8(due), events] => (
                    outcome.clone(),
                    match reason {
                        SqlValue::Text(reason) => Some(reason.clone()),
                        _ => None,
                    },
                    *due,
                    match events {
                        SqlValue::Int4(events) => Some(*events),
                        _ => None,
                    },
                ),
                other => panic!("not a run: {other:?}"),
            })
            .collect()
    }

    /// When the census is next due, as minutes after `T0`.
    async fn next_due(&self) -> i64 {
        self.count("SELECT ((extract(epoch FROM \"next_run_at\") * 1000 - 1792400400000) / 60000)::int8 FROM \"jadpo_schedules\"")
            .await
    }

    /// Each census's count of chores, and when it was taken as minutes
    /// after `T0`, oldest first.
    async fn censuses(&self) -> Vec<(i32, i64)> {
        self.rows("SELECT \"seen\"::int4, ((extract(epoch FROM \"taken_at\") * 1000 - 1792400400000) / 60000)::int8 FROM \"census\" ORDER BY \"taken_at\"")
            .await
            .into_iter()
            .map(|row| match row.as_slice() {
                [SqlValue::Int4(seen), SqlValue::Int8(at)] => (*seen, *at),
                other => panic!("not a census: {other:?}"),
            })
            .collect()
    }

    /// Each delivery's state and attempts, oldest event first.
    async fn deliveries(&self) -> Vec<(String, i32)> {
        self.rows("SELECT \"d\".\"status\", \"d\".\"attempts\" FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" ORDER BY \"o\".\"emitted_at\"")
            .await
            .into_iter()
            .map(|row| match row.as_slice() {
                [SqlValue::Text(status), SqlValue::Int4(attempts)] => (status.clone(), *attempts),
                other => panic!("not a delivery: {other:?}"),
            })
            .collect()
    }
}

/// Runs `action` with `args` as `caller`, signed in, or as no one.
async fn call(
    program: &Program,
    host: &Store,
    action: &str,
    args: Vec<Value>,
    caller: Option<&Value>,
) -> Outcome {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program);
    ctx.set_caller(caller.map(|person| Caller {
        value: person.clone(),
        key: "user".to_string(),
        session: None,
    }));
    program.run(&mut ctx, id, args).await
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

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
const MINUTE: i64 = 60_000;

#[test]
fn a_schedule_runs_on_the_tick_under_a_lease_and_the_tick_sweeps() {
    let root = project("ticks");
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
    let plan = program.schedules().expect("schedules");
    let [census] = plan.schedules.as_slice() else {
        panic!("one schedule: {plan:?}");
    };
    assert_eq!(census.every_ms, 15 * MINUTE as u64);
    assert_eq!(plan.store.as_deref(), Some("main"));
    assert_eq!(plan.stores, ["main"]);
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    // Two instances of the application, each with its own connection.
    let host = Store::new(&url, 0);
    let other = Store::new(&url, 1 << 40);
    let at = |minutes: i64| {
        let now = T0 + minutes * MINUTE;
        host.clock.set(now);
        other.clock.set(now);
        now
    };
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let join = |handle: &str| {
            let handle = handle.to_string();
            let (program, host) = (&program, &host);
            async move { value(call(program, host, "join", vec![Value::text(&handle)], None).await) }
        };
        let ann = join("ann").await;
        let bob = join("bob").await;
        let (ann_in, bob_in) = (signed_in(&ann), signed_in(&bob));
        for (person, signed, home, chore) in [
            (&ann, &ann_in, "Ash Lane", "sweep"),
            (&bob, &bob_in, "Birch Row", "mow"),
        ] {
            let household = value(
                call(&program, &host, "found", vec![Value::text(home), signed.clone()], Some(signed))
                    .await,
            );
            value(
                call(
                    &program,
                    &host,
                    "add_chore",
                    vec![field(&household, "id"), field(person, "id"), Value::text(chore)],
                    Some(signed),
                )
                .await,
            );
        }

        // The first tick that sees the schedule keeps its state: its first
        // run is due one interval later, and nothing runs yet.
        let first = tick(&program, &host, at(0)).await;
        assert!(first.due.is_empty(), "{:?}", first.due);
        assert!(first.problems.is_empty(), "{:?}", first.problems);
        assert_eq!(host.next_due().await, 15);

        // Two instances tick at once when it is due: one takes the lease and
        // runs it, the other finds it taken and does nothing.
        let now = at(15);
        let (one, two) = futures_join(tick(&program, &host, now), tick(&program, &other, now)).await;
        let ran: Vec<_> = one.due.iter().chain(&two.due).collect();
        assert_eq!(ran.len(), 1, "{ran:?}");
        assert_eq!(ran[0].ran, Ran::Succeeded { events: 1 });
        assert_eq!(ran[0].due_slot, now);
        // As the system with no tenant, the run sees both households'
        // chores, and `clock.now` is the tick's time.
        assert_eq!(host.censuses().await, [(2, 15)]);
        assert_eq!(host.runs().await, [("succeeded".to_string(), None, 15, Some(1))]);
        let traced = host
            .count("SELECT count(*) FROM \"jadpo_schedule_runs\" WHERE \"traceparent\" LIKE '00-%'")
            .await;
        assert_eq!(traced, 1, "a run starts a trace");
        assert_eq!(host.next_due().await, 30);
        // What it emitted was delivered after it: no bell is hung, and with
        // one attempt the delivery fails for good.
        assert_eq!(host.deliveries().await, [("failed".to_string(), 1)]);

        // A run still holding the lease when the next one is due: the tick
        // is skipped, recorded, and the due time moves on, not queued.
        host.rows("INSERT INTO \"jadpo_schedule_runs\" (\"id\", \"schedule\", \"due_slot\", \"started_at\", \"outcome\") VALUES ('00000000-0000-7000-8000-000000000001', 'schedule home.chore_census', '2026-10-19T09:30:00Z', '2026-10-19T09:25:00Z', 'running') RETURNING 1")
            .await;
        host.rows("UPDATE \"jadpo_schedules\" SET \"lease_until\" = '2026-10-19T09:40:00Z', \"run_id\" = '00000000-0000-7000-8000-000000000001' RETURNING 1")
            .await;
        let leased = tick(&program, &host, at(30)).await;
        assert_eq!(leased.due.len(), 1);
        assert_eq!(leased.due[0].ran, Ran::Skipped(Skipped::LeaseHeld));
        assert_eq!(host.next_due().await, 45);
        // Once its lease lapses the worker holding it is taken as lost: its
        // record says so, and the next due tick runs the schedule.
        value(call(&program, &host, "hang_bell", vec![], Some(&ann_in)).await);
        let after = tick(&program, &host, at(45)).await;
        assert_eq!(after.due[0].ran, Ran::Succeeded { events: 1 });
        let lost = host
            .rows("SELECT \"failure\" ->> 'code' FROM \"jadpo_schedule_runs\" WHERE \"id\" = '00000000-0000-7000-8000-000000000001' AND \"outcome\" = 'failed'")
            .await;
        assert_eq!(lost, [vec![SqlValue::Text("lease_expired".into())]]);
        assert_eq!(
            host.deliveries().await,
            [("failed".to_string(), 1), ("succeeded".to_string(), 1)]
        );

        // Down from 09:45 to 12:05: the ticks it missed become one run, and
        // the next due time stays on the grid.
        let caught = tick(&program, &host, at(185)).await;
        assert_eq!(caught.due.len(), 1, "{:?}", caught.due);
        assert_eq!(caught.due[0].due_slot, T0 + 60 * MINUTE);
        assert_eq!(host.next_due().await, 195);
        assert_eq!(host.censuses().await, [(2, 15), (2, 45), (2, 185)]);

        // Paused, a due tick is skipped and recorded.
        host.rows("UPDATE \"jadpo_schedules\" SET \"paused\" = TRUE RETURNING 1").await;
        let paused = tick(&program, &host, at(195)).await;
        assert_eq!(paused.due[0].ran, Ran::Skipped(Skipped::Paused));
        assert_eq!(host.next_due().await, 210);
        host.rows("UPDATE \"jadpo_schedules\" SET \"paused\" = FALSE RETURNING 1").await;
        assert_eq!(
            host.runs().await,
            [
                ("succeeded".to_string(), None, 15, Some(1)),
                ("failed".to_string(), None, 30, None),
                ("skipped".to_string(), Some("lease_held".to_string()), 30, None),
                ("succeeded".to_string(), None, 45, Some(1)),
                ("succeeded".to_string(), None, 60, Some(1)),
                ("skipped".to_string(), Some("paused".to_string()), 195, None),
            ]
        );

        // A delivery left behind by a worker that died holding it: the
        // tick's sweep delivers it once its claim has lapsed.
        host.rows("UPDATE \"jadpo_deliveries\" SET \"status\" = 'running', \"next_attempt_at\" = '2026-10-19T12:20:00Z' WHERE \"id\" = (SELECT \"d\".\"id\" FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" ORDER BY \"o\".\"emitted_at\" DESC LIMIT 1) RETURNING 1")
            .await;
        let early = tick(&program, &host, at(196)).await;
        assert!(early.due.is_empty() && early.deliveries.is_empty(), "{early:?}");
        let swept = tick(&program, &host, at(207)).await;
        assert!(swept.due.is_empty());
        assert_eq!(swept.deliveries.len(), 1, "{swept:?}");
        assert_eq!(
            host.deliveries().await,
            [
                ("failed".to_string(), 1),
                ("succeeded".to_string(), 1),
                ("succeeded".to_string(), 2),
            ]
        );

        // Expired idempotency keys are deleted; a key still kept stays.
        host.rows("INSERT INTO \"jadpo_idempotency_keys\" (\"scope\", \"key\", \"request_digest\", \"stored_at\", \"kept_until\") VALUES ('anonymous', 'old', 'd', '2026-10-19T09:00:00Z', '2026-10-20T09:00:00Z'), ('anonymous', 'fresh', 'd', '2026-10-20T12:00:00Z', '2026-10-21T12:00:00Z') RETURNING 1")
            .await;
        // A day later: the events every subscriber had are deleted with
        // their deliveries; the one whose delivery failed stays for an
        // operator, and the census this tick takes is too new to go.
        let day = tick(&program, &host, at(28 * 60)).await;
        assert_eq!(day.deleted.keys, 1);
        assert_eq!(day.deleted.events, 2);
        assert!(day.problems.is_empty(), "{:?}", day.problems);
        let keys = host.rows("SELECT \"key\" FROM \"jadpo_idempotency_keys\"").await;
        assert_eq!(keys, [vec![SqlValue::Text("fresh".into())]]);
        assert_eq!(
            host.deliveries().await,
            [("failed".to_string(), 1), ("succeeded".to_string(), 1)]
        );
        assert_eq!(host.count("SELECT count(*) FROM \"jadpo_outbox\"").await, 2);

        // Run records are kept 30 days.
        let month = tick(&program, &host, at(31 * 24 * 60)).await;
        assert_eq!(month.deleted.runs, 6, "{month:?}");
        assert_eq!(
            host.count("SELECT count(*) FROM \"jadpo_schedule_runs\"").await,
            2,
            "the day's run and this tick's"
        );
    });
}

/// Runs `a` and `b` together on this thread, as two instances ticking at
/// the same moment do.
async fn futures_join<A: std::future::Future, B: std::future::Future>(
    a: A,
    b: B,
) -> (A::Output, B::Output) {
    let mut a = std::pin::pin!(a);
    let mut b = std::pin::pin!(b);
    let (mut left, mut right) = (None, None);
    std::future::poll_fn(|cx| {
        if left.is_none()
            && let std::task::Poll::Ready(value) = a.as_mut().poll(cx)
        {
            left = Some(value);
        }
        if right.is_none()
            && let std::task::Poll::Ready(value) = b.as_mut().poll(cx)
        {
            right = Some(value);
        }
        if left.is_some() && right.is_some() {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
    (left.expect("a"), right.expect("b"))
}

// --- Watching schedules (chunk 6.2b) -----------------------------------------

/// A check of the bell each hour, which fails with `bell_missing` (of kind
/// `unavailable`) until a bell is hung.
const BELL_CHECK: (&str, &str) = (
    "domains/home/schedules/bell_check.jadpo",
    r#"use home/actions { ring }

/// Checks the bell is hung.
schedule bell_check {
    every: 1h
    run: ring()
}
"#,
);

fn watched_project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("schedules-6.2b-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES.iter().chain([&BELL_CHECK]) {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

/// `jadpo <args> --url <url>` at `at` (milliseconds), as a terminal with
/// colour off shows it, bold taken out.
fn watching(root: &Path, url: &str, at: i64, args: &[&str]) -> Output {
    let mut args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    args.extend(["--url".to_string(), url.to_string()]);
    let env = Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", &(at / 1000).to_string())
        .with_terminals(true, true, true)
        .with_var("TERM", "xterm-256color")
        .with_var("NO_COLOR", "1");
    let mut output = run(&args, &env.with_cwd(root));
    output.stdout = output.stdout.replace("\x1b[1m", "").replace("\x1b[0m", "");
    output.stderr = output.stderr.replace("\x1b[1m", "").replace("\x1b[0m", "");
    output
}

/// The screen, with a `traceparent`'s random ids written as `…`.
fn without_trace(text: &str) -> String {
    text.lines()
        .map(|line| match line.find("00-") {
            Some(at) if line.trim_start().starts_with("trace") => format!("{}00-…", &line[..at]),
            _ => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn schedules_are_listed_explained_paused_resumed_run_and_paged() {
    let root = watched_project("watch");
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
    let host = Store::new(&url, 0);
    let at = |minutes: i64, seconds: i64| T0 + minutes * MINUTE + seconds * 1000;
    let runtime = Runtime::new().expect("runtime");
    let tick_at = |minutes: i64| {
        host.clock.set(at(minutes, 0));
        runtime.block_on(tick(&program, &host, at(minutes, 0)))
    };

    // No tick has seen either schedule yet.
    let listed = watching(&root, &url, at(0, 0) - 5_000, &["schedules"]);
    assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
    assert_eq!(
        listed.stdout,
        "  schedule      last run (UTC)  next
  bell_check    – none yet      –
  chore_census  – none yet      –

✓ 2 schedules  no tick yet
"
    );
    let why = watching(&root, &url, at(0, 0), &["why", "chore_census"]);
    assert_eq!(why.code, 0, "{}{}", why.stdout, why.stderr);
    assert_eq!(
        why.stdout,
        "No tick has seen `chore_census` yet: the app hasn't run since it was added
  schedule  domains/home/schedules/chore_census.jadpo:3
"
    );

    // The first tick keeps their state; neither is due yet.
    tick_at(0);
    let listed = watching(&root, &url, at(0, 30), &["schedules"]);
    assert_eq!(
        listed.stdout,
        "  schedule      last run (UTC)  next
  bell_check    – none yet      10:00
  chore_census  – none yet      09:15

✓ 2 schedules  last tick 30 s ago
"
    );
    let history = watching(
        &root,
        &url,
        at(0, 30),
        &["schedules", "history", "chore_census"],
    );
    assert_eq!(
        history.stdout,
        "`chore_census` hasn't been due yet: its first run is due at 09:15\n"
    );

    // An hour on: the census has run four times, and the bell check
    // failed.
    for minutes in [15, 30, 45, 60] {
        tick_at(minutes);
    }
    let listed = watching(&root, &url, at(60, 20), &["schedules"]);
    assert_eq!(listed.code, 0);
    assert_eq!(
        listed.stdout,
        "  schedule      last run (UTC)                          next
  bell_check    ✗ failed     10:00:00 · bell_missing    11:00
  chore_census  ✓ succeeded  10:00:00 · 0 ms · 1 event  10:15

✗ 1 failed  2 schedules · last tick 20 s ago

→ jadpo why bell_check  explains what went wrong
"
    );
    let why = watching(&root, &url, at(60, 20), &["why", "bell_check"]);
    assert_eq!(
        without_trace(&why.stdout),
        "`bell_check` failed at 10:00:00 with bell_missing
  run       due 10:00 · took 0 ms
  kind      unavailable
  next      11:00: a schedule's next run is its retry
  trace     00-…
  schedule  domains/home/schedules/bell_check.jadpo:3

→ jadpo schedules run bell_check  runs it again now, once the cause is fixed
"
    );
    let json = watching(&root, &url, at(60, 20), &["why", "bell_check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(document["data"]["answer"], "failed");
    assert_eq!(
        document["data"]["place"],
        "domains/home/schedules/bell_check.jadpo:3"
    );

    // Three failures in a row, and no success in twice its interval: two
    // alerts, and the footer says so.
    for minutes in [75, 90, 105, 120, 135, 150, 165, 180] {
        tick_at(minutes);
    }
    let listed = watching(&root, &url, at(180, 20), &["schedules"]);
    assert_eq!(
        listed.stdout,
        "  schedule      last run (UTC)                                     next
  bell_check    ✗ failed     12:00:00 · bell_missing · 3 in a row  13:00
  chore_census  ✓ succeeded  12:00:00 · 0 ms · 1 event             12:15

▲ alert  `bell_check` has failed 3 times in a row
▲ alert  `bell_check` hasn't succeeded since it was first due at 10:00

✗ 1 failed  2 schedules · 2 alerts · last tick 20 s ago

→ jadpo why bell_check  explains what went wrong
"
    );
    let json = watching(&root, &url, at(180, 20), &["schedules", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(
        document["data"]["alerts"][0],
        serde_json::json!({
            "event": "schedule.alert",
            "alert": "failed_in_a_row",
            "schedule": "bell_check",
            "failures": 3,
        })
    );

    // Paused: it says so, again is no change, and its due tick is skipped.
    let paused = watching(
        &root,
        &url,
        at(180, 30),
        &["schedules", "pause", "chore_census"],
    );
    assert_eq!(paused.code, 0, "{}{}", paused.stdout, paused.stderr);
    assert_eq!(
        paused.stdout,
        "✓ Paused `chore_census`: its due ticks are skipped until it is resumed

→ jadpo schedules resume chore_census  starts it again
"
    );
    let again = watching(
        &root,
        &url,
        at(180, 40),
        &["schedules", "pause", "chore_census"],
    );
    assert_eq!(
        again.stdout,
        "✓ `chore_census` was already paused

→ jadpo schedules resume chore_census  starts it again
"
    );
    tick_at(195);
    let listed = watching(&root, &url, at(195, 10), &["schedules"]);
    assert!(
        listed.stdout.contains(
            "  chore_census  ✓ succeeded  12:00:00 · 0 ms · 1 event             ‖ paused\n"
        ),
        "{}",
        listed.stdout
    );
    assert!(listed.stdout.contains("· 1 paused ·"), "{}", listed.stdout);
    let why = watching(&root, &url, at(195, 10), &["why", "chore_census"]);
    assert_eq!(
        why.stdout,
        "`chore_census` is paused, so its due ticks are skipped
  skipped   1 due time since 12:15
  last run  succeeded at 12:00:00
  schedule  domains/home/schedules/chore_census.jadpo:3

→ jadpo schedules resume chore_census  starts it again
"
    );
    let resumed = watching(
        &root,
        &url,
        at(195, 20),
        &["schedules", "resume", "chore_census"],
    );
    assert_eq!(
        resumed.stdout,
        "✓ Resumed `chore_census`: its next run is at 12:30\n"
    );

    // Run now, by hand, under the lease: in this process, against the
    // same database.
    let ran = watching(
        &root,
        &url,
        at(196, 0),
        &["schedules", "run", "chore_census"],
    );
    assert_eq!(ran.code, 0, "{}{}", ran.stdout, ran.stderr);
    assert!(
        ran.stdout.starts_with("✓ Ran `chore_census` in ")
            && ran.stdout.ends_with(", emitting 1 event\n"),
        "{}",
        ran.stdout
    );
    let failed = watching(
        &root,
        &url,
        at(196, 30),
        &["schedules", "run", "bell_check"],
    );
    assert_eq!(failed.code, 1, "{}{}", failed.stdout, failed.stderr);
    assert!(
        failed.stdout.starts_with(
            "✗ failed  `bell_check`\n  failure   bell_missing · unavailable\n  took      "
        ),
        "{}",
        failed.stdout
    );
    assert!(
        failed
            .stdout
            .ends_with("\n\n→ jadpo why bell_check  explains the run\n"),
        "{}",
        failed.stdout
    );

    // A run still holding the lease: running, and a run now is refused.
    tick_at(200);
    runtime.block_on(async {
        host.rows("INSERT INTO \"jadpo_schedule_runs\" (\"id\", \"schedule\", \"due_slot\", \"started_at\", \"outcome\") VALUES ('00000000-0000-7000-8000-0000000000aa', 'schedule chore_census', '2026-10-19T12:20:00Z', '2026-10-19T12:20:00Z', 'running') RETURNING 1")
            .await;
        host.rows("UPDATE \"jadpo_schedules\" SET \"lease_until\" = '2026-10-19T12:25:00Z', \"run_id\" = '00000000-0000-7000-8000-0000000000aa' WHERE \"schedule\" = 'schedule chore_census' RETURNING 1")
            .await;
    });
    let listed = watching(&root, &url, at(200, 40), &["schedules"]);
    assert!(
        listed
            .stdout
            .contains("  chore_census  ● running  12:20:00 · 40 s so far"),
        "{}",
        listed.stdout
    );
    let refused = watching(
        &root,
        &url,
        at(200, 40),
        &["schedules", "run", "chore_census"],
    );
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(
        refused
            .stderr
            .contains("✗ error  `chore_census` is already running"),
        "{}",
        refused.stderr
    );
    assert!(
        refused.stderr.contains("rule      schedules.lease_held"),
        "{}",
        refused.stderr
    );
    let why = watching(&root, &url, at(200, 40), &["why", "chore_census"]);
    assert_eq!(
        why.stdout,
        "`chore_census`'s run from 12:20:00 is still going, so its due ticks are skipped
until it ends
  lease     held until 12:25:00, the 15-minute limit and a minute
  schedule  domains/home/schedules/chore_census.jadpo:3
"
    );
    // Past its lease's end, its worker was lost.
    host.clock.set(at(205, 30));
    runtime.block_on(tick(&program, &host, at(205, 30)));
    let why = watching(&root, &url, at(206, 0), &["why", "chore_census"]);
    assert!(
        why.stdout.starts_with(
            "`chore_census`'s run from 12:20:00 held its lease past 12:25:00: its worker was\nlost\n"
        ),
        "{}",
        why.stdout
    );

    // The tick takes it over, and history pages through the records.
    for minutes in (225..=360).step_by(15) {
        tick_at(minutes);
    }
    let history = watching(
        &root,
        &url,
        at(360, 30),
        &["schedules", "history", "chore_census"],
    );
    assert_eq!(history.code, 0, "{}{}", history.stdout, history.stderr);
    assert!(
        history.stdout.starts_with(
            "2026-10-19
  due (UTC)  started    took  outcome
  15:00      15:00:00   0 ms  ✓ succeeded  1 event
"
        ),
        "{}",
        history.stdout
    );
    assert!(
        history
            .stdout
            .contains("\n  12:20      12:20:00  5 min  ✗ failed     lease_expired\n")
            && history.stdout.contains("\n  –          12:16:00  ")
            && history.stdout.contains("✓ succeeded  1 event · by hand\n")
            && history
                .stdout
                .contains("\n  12:15      –             –  – skipped    paused\n"),
        "{}",
        history.stdout
    );
    assert!(
        history
            .stdout
            .contains("  runs      24 in the last 30 days · 1 failed · 1 skipped tick\n  took      median 0 ms · longest 5 min\n  late      median 0 ms · at most 15 min\n"),
        "{}",
        history.stdout
    );
    assert!(
        history.stdout.ends_with(
            "→ jadpo schedules history chore_census --page 2  shows the 5 before these\n"
        ),
        "{}",
        history.stdout
    );
    let second = watching(
        &root,
        &url,
        at(360, 30),
        &["schedules", "history", "chore_census", "--page", "2"],
    );
    assert!(
        second
            .stdout
            .contains("\n  09:15      09:15:00  0 ms  ✓ succeeded  1 event\n")
            && second.stdout.ends_with("at most 15 min\n"),
        "{}",
        second.stdout
    );
    let past = watching(
        &root,
        &url,
        at(360, 30),
        &["schedules", "history", "chore_census", "--page", "3"],
    );
    assert_eq!(past.code, 2);
    assert!(
        past.stderr.contains("`chore_census` has no page 3"),
        "{}",
        past.stderr
    );

    // Hours with no tick: the ticks have stopped, and the next due time
    // is late.
    let listed = watching(&root, &url, at(480, 0), &["schedules"]);
    assert!(
        listed
            .stdout
            .contains("▲ alert  No tick has arrived since 15:00:00: the app isn't running\n"),
        "{}",
        listed.stdout
    );
    assert!(
        listed.stdout.contains("▲ late since 15:15"),
        "{}",
        listed.stdout
    );
    let why = watching(&root, &url, at(480, 0), &["why", "chore_census"]);
    assert_eq!(
        why.stdout,
        "No tick has reached `chore_census` since 15:00:00
  due       15:15, not run
  ticks     none since 15:00:00; the running app ticks every 1 min
  schedule  domains/home/schedules/chore_census.jadpo:3

▲ alert  `chore_census` hasn't succeeded since 15:00:00, over twice its `15m`
"
    );

    // A misspelt name gets the nearest.
    let misspelt = watching(&root, &url, at(480, 0), &["why", "chore_censu"]);
    assert_eq!(misspelt.code, 2);
    assert!(
        misspelt
            .stderr
            .contains("Use `chore_census` instead of `chore_censu`"),
        "{}",
        misspelt.stderr
    );
}

#[test]
fn a_project_with_no_schedules_says_so() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("schedules-6.2b-none");
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    let listed = watching(
        &root,
        "postgresql://unused.invalid/none",
        T0,
        &["schedules"],
    );
    assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
    assert_eq!(listed.stdout, "The project declares no schedules\n");
    let why = watching(
        &root,
        "postgresql://unused.invalid/none",
        T0,
        &["why", "census"],
    );
    assert_eq!(why.code, 2);
    assert!(
        why.stderr.contains("`census` isn't a schedule"),
        "{}",
        why.stderr
    );
}

/// A project with two schedules a second apart: `heartbeat` makes a pulse
/// each second, `missed_beat` fails each second.
const BEATS: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/beats/entities/pulse.jadpo",
        r#"entity Pulse {
    id: Uuid

    policy: {
        create: [Access.system]
        read: [Access.system]
        tenant: Tenancy.shared
    }
}
"#,
    ),
    (
        "domains/beats/actions/beat.jadpo",
        r#"use beats/entities { Pulse }

action beat() -> Pulse {
    return Pulse.create {
        set: {}
    }
}
"#,
    ),
    (
        "domains/beats/failures/skipped_beat.jadpo",
        r#"type SkippedBeat = Failure {
    kind: FailureKind.unavailable
    message: "The beat was skipped."
}
"#,
    ),
    (
        "domains/beats/actions/miss.jadpo",
        r#"use beats/failures { SkippedBeat }

action miss() fails SkippedBeat -> Unit {
    reject SkippedBeat {}
}
"#,
    ),
    (
        "domains/beats/schedules/heartbeat.jadpo",
        r#"use beats/actions { beat }

/// Beats every second.
schedule heartbeat {
    every: 1s
    run: beat()
}
"#,
    ),
    (
        "domains/beats/schedules/missed_beat.jadpo",
        r#"use beats/actions { miss }

/// Misses every second.
schedule missed_beat {
    every: 1s
    run: miss()
}
"#,
    ),
];

/// What a session writes, shared with the thread that runs it.
#[derive(Clone, Default)]
struct Written(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Written {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("written").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Written {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("written")).into_owned()
    }
}

#[test]
fn jadpo_dev_prints_each_run_and_each_alert_once() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("schedules-6.2b-dev");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in BEATS {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().expect("a PostgreSQL server");
    let url = server.create_database("dev_6_2b").expect("a database");
    let out = Written::default();
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let args = jadpo_cli::dev::DevArgs {
        cwd: root.clone(),
        port: 0,
        url: Some(url),
        no_run: false,
        json: false,
        choice: jadpo_term::ColorChoice::Never,
    };
    let session = {
        let mut out = out.clone();
        let stop = std::sync::Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut err = Written::default();
            jadpo_cli::dev::serve_to(&args, &Environment::empty(), &mut out, &mut err, &|| {
                stop.load(Ordering::SeqCst)
            })
        })
    };
    let started = std::time::Instant::now();
    let failed_alert = "alert: `missed_beat` has failed 3 times in a row";
    let text = loop {
        let text = out.text();
        let beats = text.matches("schedule heartbeat").count();
        if text.contains(failed_alert) && beats >= 5 {
            break text;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "no runs or alerts yet:\n{text}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    stop.store(true, Ordering::SeqCst);
    assert_eq!(session.join().expect("the session"), 0);
    // Each run is a line: its trace's id, the schedule, its outcome, what
    // it emitted or why it failed, and its time.
    let beat = text
        .lines()
        .find(|line| line.contains("schedule heartbeat"))
        .expect("a heartbeat");
    let (id, rest) = beat.split_at(8);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "{beat}");
    assert!(
        rest.starts_with("  schedule heartbeat                 succeeded 0 events   "),
        "{beat}"
    );
    assert!(beat.ends_with(" ms") || beat.ends_with(" s"), "{beat}");
    let missed = text
        .lines()
        .find(|line| line.contains("schedule missed_beat"))
        .expect("a missed beat");
    assert!(
        missed[8..].starts_with(
            "  schedule missed_beat               failed skipped_beat · unavailable   "
        ),
        "{missed}"
    );
    // An alert prints once, when it starts.
    assert_eq!(text.matches(failed_alert).count(), 1, "{text}");
}
