//! Events against live PostgreSQL (area 12, "Emitting an event",
//! "Delivery: at least once, safe to repeat" and "Retries and giving
//! up"; chunk 6.1b): an emitted event is kept in the outbox in the same
//! transaction as the action's writes, with a delivery record for each
//! subscriber and the request's trace context; it is delivered after the
//! response, each subscriber as the system limited to the event's
//! tenant; a subscriber failing `unavailable` is retried after a backoff
//! until its budget runs out; what a crash between the commit and the
//! delivery left is picked up by the sweep; and a repeated delivery
//! finds its handled record and does nothing.
//!
//! A household application: adding a chore emits `ChoreAdded`, which
//! `tally_chores` counts the household's chores for, and `ring_bell`
//! rings a bell for, failing until one is hung.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::outbox::Trace;
use jadpo_runtime::runner::{Deferred, run_deferred, sweep};
use jadpo_runtime::{Caller, Executor, Outcome, SqlType, SqlValue};
use jadpo_std::http::PrincipalKind;
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
use home/events { ChoreAdded }
use home/types { ChoreView, Title }

action add_chore(household_id: Household.id, owner_id: Person.id, title: Title) fails HouseholdNotFound, PersonNotFound -> ChoreView {
    var chore: Chore = attempt Chore.create {
        set: {
            household_id: household_id
            owner_id: owner_id
            title: title
        }
    }
    emit ChoreAdded {
        chore_id: chore.id
        household_id: chore.household_id
    }
    return ChoreView {
        id: chore.id
        title: chore.title
    }
}
"#,
    ),
    (
        "domains/home/actions/count_chores.jadpo",
        r#"use home/entities { Household, HouseholdNotFound, Tally }
use home/queries { chores }
use home/types { ChoreView, Count }

action count_chores(household_id: Household.id) fails HouseholdNotFound -> Unit {
    var seen: List<ChoreView> = chores()
    var tally: Tally = attempt Tally.create {
        set: {
            household_id: household_id
            seen: Count(list.count(items: seen))
        }
    }
    assert tally.seen >= 0
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
        "domains/home/entities/tally.jadpo",
        r#"use home/entities { Household }
use home/types { Count }

entity Tally {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    seen: Count

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
"#,
    ),
    (
        "domains/home/events/chore_added.jadpo",
        r#"use home/entities { Chore, Household }

/// A chore was added to a household.
type ChoreAdded = Event {
    chore_id: Chore.id
    household_id: Household.id
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
        "domains/home/subscribers/ring_bell.jadpo",
        r#"use home/actions { ring }
use home/events { ChoreAdded }

/// Rings the bell for each new chore, once one is hung.
subscriber ring_bell {
    on: ChoreAdded
    run: ring()
    retry: {
        attempts: 3
        within: 1h
    }
}
"#,
    ),
    (
        "domains/home/subscribers/tally_chores.jadpo",
        r#"use home/actions { count_chores }
use home/events { ChoreAdded }

/// Counts the chores the system sees in the household a chore was added to.
subscriber tally_chores {
    on: ChoreAdded
    run: count_chores(event.household_id)
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
        "domains/home/types/title.jadpo",
        r#"type Title = Text {
    min_length: 1
    max_length: 80
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("events-{name}"));
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

/// Each delivery of `subscriber`'s state, attempts and last failure's
/// code, oldest event first.
async fn deliveries(host: &Harness, subscriber: &str) -> Vec<(String, i32, Option<String>)> {
    let sql = format!(
        "SELECT \"d\".\"status\", \"d\".\"attempts\", \"d\".\"last_failure\" ->> 'code' FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" WHERE \"d\".\"subscriber\" LIKE '%{subscriber}' ORDER BY \"o\".\"emitted_at\", \"o\".\"id\""
    );
    host.rows(&sql)
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(status), SqlValue::Int4(attempts), code] => (
                status.clone(),
                *attempts,
                match code {
                    SqlValue::Text(code) => Some(code.clone()),
                    _ => None,
                },
            ),
            other => panic!("not a delivery: {other:?}"),
        })
        .collect()
}

/// Each tally's household's name and how many chores it saw, by name.
async fn tallies(host: &Harness) -> Vec<(String, i32)> {
    host.rows("SELECT \"h\".\"name\", \"t\".\"seen\"::int4 FROM \"tally\" AS \"t\" JOIN \"household\" AS \"h\" ON \"h\".\"id\" = \"t\".\"household_id\" ORDER BY \"h\".\"name\"")
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(name), SqlValue::Int4(seen)] => (name.clone(), *seen),
            other => panic!("not a tally: {other:?}"),
        })
        .collect()
}

/// Runs `action` with `args` as `caller`, signed in, or as no one, and
/// gives back its outcome and the work it left for after its response.
async fn call(
    program: &Program,
    host: &Harness,
    action: &str,
    args: Vec<Value>,
    caller: Option<&Value>,
    trace: Option<Trace>,
) -> (Outcome, Vec<Deferred>) {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program.places());
    ctx.set_caller(caller.map(|person| Caller {
        value: person.clone(),
        key: "user".to_string(),
        session: None,
    }));
    ctx.set_trace(trace);
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

/// The request's trace context, as a caller's `traceparent` sends it.
const TRACEPARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;

#[test]
fn an_event_is_kept_with_its_writes_and_delivered_after_the_response_retried_and_swept() {
    let root = project("deliveries");
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
    let events = program.runtime_plan().events().expect("subscribers");
    assert_eq!(events.plan.subscribers.len(), 2);
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url).at(T0).random_from(0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let join = |handle: &str| {
            let handle = handle.to_string();
            let (program, host) = (&program, &host);
            async move { value(call(program, host, "join", vec![Value::text(&handle)], None, None).await.0) }
        };
        let ann = join("ann").await;
        let bob = join("bob").await;
        let (ann_in, bob_in) = (signed_in(&ann), signed_in(&bob));
        let found = |name: &str, by: &Value| {
            let (name, by) = (name.to_string(), by.clone());
            let (program, host) = (&program, &host);
            async move {
                value(
                    call(program, host, "found", vec![Value::text(&name), by.clone()], Some(&by), None)
                        .await
                        .0,
                )
            }
        };
        let ash = found("Ash Lane", &ann_in).await;
        let birch = found("Birch Row", &bob_in).await;

        // Adding a chore emits `ChoreAdded`: kept in the outbox in the
        // action's own transaction, with the request's trace context and a
        // pending delivery for each subscriber, and delivered to nobody
        // while the request is answered.
        let request = Trace::parse(TRACEPARENT).expect("a trace");
        let (added, after_sweep) = call(
            &program,
            &host,
            "add_chore",
            vec![field(&ash, "id"), field(&ann, "id"), Value::text("sweep")],
            Some(&ann_in),
            Some(request),
        )
        .await;
        value(added);
        let outbox = host
            .rows("SELECT \"event\", \"traceparent\", \"encoding\"::int4, \"depth\", \"payload\" ->> 'household_id' FROM \"jadpo_outbox\"")
            .await;
        let [row] = outbox.as_slice() else {
            panic!("one event: {outbox:?}");
        };
        match row.as_slice() {
            [
                SqlValue::Text(event),
                SqlValue::Text(traceparent),
                SqlValue::Int4(1),
                SqlValue::Int4(0),
                SqlValue::Text(household),
            ] => {
                assert!(event.ends_with("ChoreAdded"), "{event}");
                assert_eq!(traceparent, TRACEPARENT, "the emitting request's trace context");
                assert_eq!(Value::uuid(household), field(&ash, "id"), "the payload is its JSON");
            }
            other => panic!("not the outbox row: {other:?}"),
        }
        let pending = ("pending".to_string(), 0, None);
        assert_eq!(deliveries(&host, "tally_chores").await, vec![pending.clone()]);
        assert_eq!(deliveries(&host, "ring_bell").await, vec![pending]);
        assert_eq!(tallies(&host).await, [], "nothing delivered in the request");

        // An action that fails emits nothing: its outbox row rolls back
        // with it.
        let (refused, nothing) = call(
            &program,
            &host,
            "add_chore",
            vec![field(&ash, "id"), field(&bob, "id"), Value::text("dust")],
            Some(&bob_in),
            None,
        )
        .await;
        assert!(matches!(refused, Outcome::Failed(_)), "{refused:?}");
        assert!(nothing.is_empty());
        assert_eq!(host.count("SELECT count(*)::int4 FROM \"jadpo_outbox\"").await, 1);

        // After the response: each subscriber delivered on its own, as the
        // system limited to the event's household. The tally succeeds; the
        // bell, with none hung, fails `unavailable` and waits to retry.
        let ran = run_deferred(&program, &host, after_sweep).await;
        assert_eq!(ran.len(), 2, "{ran:?}");
        assert_eq!(tallies(&host).await, [("Ash Lane".to_string(), 1)]);
        assert_eq!(
            deliveries(&host, "tally_chores").await,
            [("succeeded".to_string(), 1, None)]
        );
        assert_eq!(
            deliveries(&host, "ring_bell").await,
            [("pending".to_string(), 1, Some("bell_missing".to_string()))]
        );
        let next = host
            .count("SELECT (extract(epoch FROM \"next_attempt_at\") * 1000 - 1792400400000)::int4 FROM \"jadpo_deliveries\" WHERE \"status\" = 'pending'")
            .await;
        assert!((1_000..=30_000).contains(&next), "the first wait is at most 30 s: {next}");
        assert_eq!(
            host.count("SELECT count(*)::int4 FROM \"jadpo_handled_deliveries\"").await,
            1,
            "the tally's handled record committed with its write"
        );

        // A chore added in another household, and the process stops before
        // its work runs: the deliveries stay pending, and the sweep picks
        // them up. Its tally sees only its own household's chore, though
        // there are two.
        let (added, dropped) = call(
            &program,
            &host,
            "add_chore",
            vec![field(&birch, "id"), field(&bob, "id"), Value::text("mow")],
            Some(&bob_in),
            None,
        )
        .await;
        value(added);
        drop(dropped);
        host.set_now(T0 + 31_000);
        let swept = sweep(&program, &host, "main", T0 + 31_000)
            .await
            .expect("the sweep runs");
        assert_eq!(swept.len(), 3, "both bells and the new tally: {swept:?}");
        assert_eq!(
            tallies(&host).await,
            [("Ash Lane".to_string(), 1), ("Birch Row".to_string(), 1)],
            "each subscriber reaches only its event's household"
        );
        assert_eq!(
            deliveries(&host, "ring_bell").await,
            [
                ("pending".to_string(), 2, Some("bell_missing".to_string())),
                ("pending".to_string(), 1, Some("bell_missing".to_string())),
            ]
        );
        // Nothing is due again until its wait passes.
        let early = sweep(&program, &host, "main", T0 + 32_000).await.expect("runs");
        assert!(early.is_empty(), "{early:?}");

        // Its budget is three attempts: the first bell's third fails for good.
        host.set_now(T0 + 300_000);
        sweep(&program, &host, "main", T0 + 300_000).await.expect("runs");
        assert_eq!(
            deliveries(&host, "ring_bell").await,
            [
                ("failed".to_string(), 3, Some("bell_missing".to_string())),
                ("pending".to_string(), 2, Some("bell_missing".to_string())),
            ]
        );

        // Once a bell is hung, the retry succeeds.
        value(call(&program, &host, "hang_bell", vec![], Some(&ann_in), None).await.0);
        host.set_now(T0 + 1_800_000);
        sweep(&program, &host, "main", T0 + 1_800_000).await.expect("runs");
        assert_eq!(
            deliveries(&host, "ring_bell").await,
            [
                ("failed".to_string(), 3, Some("bell_missing".to_string())),
                ("succeeded".to_string(), 3, None),
            ]
        );

        // A delivery repeated, as after a crash between its commit and its
        // record: the handled record is found, and nothing is written twice.
        host.rows("UPDATE \"jadpo_deliveries\" SET \"status\" = 'pending', \"next_attempt_at\" = '2026-10-19T09:00:00Z' WHERE \"subscriber\" LIKE '%tally_chores' RETURNING \"id\"")
            .await;
        let repeated = sweep(&program, &host, "main", T0 + 1_800_000).await.expect("runs");
        assert_eq!(repeated.len(), 2);
        assert_eq!(tallies(&host).await.len(), 2, "no second tally");
        assert_eq!(
            deliveries(&host, "tally_chores").await,
            [
                ("succeeded".to_string(), 2, None),
                ("succeeded".to_string(), 2, None),
            ]
        );
        let _ = (SqlType::Uuid, &events);
    });
}

#[test]
fn a_payload_is_read_by_its_encoding_and_a_later_one_is_parked() {
    // Chunk 6.8: a delivery reads its event's payload by the encoding its
    // outbox row records (area 19, "Changing a standard type's
    // encoding"): this release's, or none, a payload kept before
    // encodings were recorded, whose forms are encoding 1's. One a later
    // release wrote is never guessed at: parked with its problem, as a
    // payload that no longer reads is.
    let root = project("encodings-6-8");
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
    let host = Harness::postgres(&url).at(T0).random_from(0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let ann = value(call(&program, &host, "join", vec![Value::text("ann")], None, None).await.0);
        let ann_in = signed_in(&ann);
        let ash = value(
            call(&program, &host, "found", vec![Value::text("Ash Lane"), ann_in.clone()], Some(&ann_in), None)
                .await
                .0,
        );
        // Two chores, their work dropped as a crash would: one event kept
        // before encodings were recorded, one a later release wrote.
        for (title, encoding) in [("sweep", "NULL"), ("mop", "2")] {
            let (added, dropped) = call(
                &program,
                &host,
                "add_chore",
                vec![field(&ash, "id"), field(&ann, "id"), Value::text(title)],
                Some(&ann_in),
                None,
            )
            .await;
            value(added);
            drop(dropped);
            host.rows(&format!(
                "UPDATE \"jadpo_outbox\" SET \"encoding\" = {encoding} WHERE \"encoding\" = 1 RETURNING \"id\""
            ))
            .await;
        }
        host.set_now(T0 + 1_000);
        sweep(&program, &host, "main", T0 + 1_000)
            .await
            .expect("the sweep runs");
        let parked = host
            .rows("SELECT COALESCE(\"o\".\"encoding\"::text, 'none'), \"d\".\"status\", \"d\".\"last_failure\" ->> 'code', \"d\".\"last_failure\" ->> 'problem' FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" WHERE \"d\".\"subscriber\" LIKE '%tally_chores' ORDER BY 1")
            .await;
        let text = |value: &SqlValue| match value {
            SqlValue::Text(text) => Some(text.clone()),
            _ => None,
        };
        let parked: Vec<Vec<Option<String>>> = parked
            .iter()
            .map(|row| row.iter().map(text).collect())
            .collect();
        assert_eq!(
            parked,
            [
                vec![
                    Some("2".to_string()),
                    Some("failed".to_string()),
                    Some("payload_unreadable".to_string()),
                    Some("written in encoding 2, which this release doesn't read".to_string()),
                ],
                vec![Some("none".to_string()), Some("succeeded".to_string()), None, None],
            ]
        );
    });
}
