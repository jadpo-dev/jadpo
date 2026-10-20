//! The system caller against live PostgreSQL (area 9, `Access.system`;
//! area 12; chunk 6.1a): what the runner runs as the system, a schedule's
//! run or a subscriber's delivery, is let in exactly where a grant names
//! `Access.system`, within the tenant a subscriber's event came from or
//! every tenant for a schedule's run, and a signed-in caller never gets
//! what only the system is granted.
//!
//! A household application: chores are read by their owner and the
//! system, changed only by the system, deleted only by their owner; an
//! action listed for `Access.system` alone is the system's to run, and
//! decides its own body's writes wherever it is called from.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::runner::{Job, as_system as run_as_system};
use jadpo_runtime::{Caller, Executor, Outcome, SqlType, SqlValue, System};
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
        home.tidy: [Access.system]
        home.close_chore: [Access.system]
    }
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
        "domains/home/types/title.jadpo",
        r#"type Title = Text {
    min_length: 1
    max_length: 80
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
        "domains/home/types/chore_view.jadpo",
        r#"use home/entities { Chore }

type ChoreView = Output {
    pick: [Chore.id, Chore.title, Chore.done]
}
"#,
    ),
    (
        "domains/home/types/tidied.jadpo",
        r#"type Tidied = Output {
    ok: Bool
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
    done: Bool

    policy: {
        create: [HomeRole.member]
        read: [owner_id, Access.system]
        update: [Access.system]
        delete: [owner_id]
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
        r#"use home/entities { Chore, Household, HouseholdNotFound, Person, PersonNotFound }
use home/types { ChoreView, Title }

action add_chore(household_id: Household.id, owner_id: Person.id, title: Title) fails HouseholdNotFound, PersonNotFound -> ChoreView {
    var chore: Chore = attempt Chore.create {
        set: {
            household_id: household_id
            owner_id: owner_id
            title: title
            done: false
        }
    }
    return ChoreView {
        id: chore.id
        title: chore.title
        done: chore.done
    }
}
"#,
    ),
    (
        "domains/home/actions/seen_chores.jadpo",
        r#"use home/queries { chores }
use home/types { ChoreView }

action seen_chores() -> List<ChoreView> {
    return chores()
}
"#,
    ),
    (
        "domains/home/actions/mark_done.jadpo",
        r#"use home/entities { Chore, ChoreNotFound }
use home/types { ChoreView }

action mark_done(chore_id: Chore.id) fails ChoreNotFound -> ChoreView {
    var chore: Chore = attempt Chore.update(chore_id) {
        set: {
            done: true
        }
    }
    return ChoreView {
        id: chore.id
        title: chore.title
        done: chore.done
    }
}
"#,
    ),
    (
        "domains/home/actions/drop_chore.jadpo",
        r#"use home/entities { Chore, ChoreNotFound }

action drop_chore(chore_id: Chore.id) fails ChoreNotFound -> Unit {
    attempt Chore.delete(chore_id)
}
"#,
    ),
    (
        "domains/home/actions/close_chore.jadpo",
        r#"use home/entities { Chore, ChoreNotFound }

action close_chore(chore_id: Chore.id) fails ChoreNotFound -> Unit {
    attempt Chore.delete(chore_id)
}
"#,
    ),
    (
        "domains/home/actions/settle_chore.jadpo",
        r#"use home/actions { close_chore }
use home/entities { Chore, ChoreNotFound }

action settle_chore(chore_id: Chore.id) fails ChoreNotFound -> Unit {
    attempt close_chore(chore_id)
}
"#,
    ),
    (
        "domains/home/actions/tidy.jadpo",
        r#"use home/types { Tidied }

action tidy() -> Tidied {
    return Tidied {
        ok: true
    }
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
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("system-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

/// Runs `action` with `args` as `caller`, signed in, or as no one.
async fn call(
    program: &Program,
    host: &Harness,
    action: &str,
    args: Vec<Value>,
    caller: Option<&Value>,
) -> Outcome {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program.places());
    ctx.set_caller(caller.map(|person| Caller {
        value: person.clone(),
        key: "user".to_string(),
        session: None,
    }));
    program.run(&mut ctx, id, args).await
}

/// Runs `action` with `args` as the system, limited to `tenant`, as the
/// runner runs a subscriber's delivery or a schedule's run.
async fn as_system(
    program: &Program,
    host: &Harness,
    action: &str,
    args: Vec<Value>,
    tenant: Option<&Value>,
) -> Outcome {
    let callable = program.plan().find(action).expect("the action");
    let tenant = tenant.map(|household| {
        match jadpo_runtime::to_sql(&field(household, "id"), SqlType::Uuid) {
            Ok(SqlValue::Uuid(bytes)) => bytes,
            other => panic!("not an identity: {other:?}"),
        }
    });
    let mut ctx = host.ctx(program.places());
    let job = Job {
        callable,
        args,
        system: System { tenant },
    };
    run_as_system(program, &mut ctx, job).await
}

fn value(outcome: Outcome) -> Value {
    match outcome {
        Outcome::Value(value) => value,
        other => panic!("expected a value, got {other:?}"),
    }
}

fn code(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Failed(failure) => failure.code.clone(),
        other => format!("{other:?}"),
    }
}

fn field(record: &Value, name: &str) -> Value {
    match record {
        Value::Record(record) => record.get(name).cloned().expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

/// The titles of the chores `seen` lists, in order.
fn titles(seen: Value) -> Vec<String> {
    match seen {
        Value::List(items) => items
            .iter()
            .map(|item| match field(item, "title") {
                Value::Text(text) => text.to_string(),
                other => panic!("not text: {other:?}"),
            })
            .collect(),
        other => panic!("not a list: {other:?}"),
    }
}

fn signed_in(person: &Value) -> Value {
    jadpo_runtime::http::identity::principal(
        PrincipalKind::User,
        field(person, "id"),
        field(person, "handle"),
    )
}

#[test]
fn the_system_is_let_in_exactly_where_access_system_grants_and_no_caller_is() {
    let root = project("chores");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    // The permissions report says what background work reaches.
    let report = fs::read_to_string(root.join("build/report/permissions.md")).expect("the report");
    assert!(
        report.contains(
            "`Access.system` holds no membership: a subscriber reaches only the `Household` its event came from, and a schedule every `Household`."
        ),
        "{report}"
    );
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
    let host = Harness::postgres(&url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let ann = value(call(&program, &host, "join", vec![Value::text("ann")], None).await);
        let bob = value(call(&program, &host, "join", vec![Value::text("bob")], None).await);
        let (ann_in, bob_in) = (signed_in(&ann), signed_in(&bob));
        let ash = value(
            call(
                &program,
                &host,
                "found",
                vec![Value::text("Ash Lane"), ann_in.clone()],
                Some(&ann_in),
            )
            .await,
        );
        let birch = value(
            call(
                &program,
                &host,
                "found",
                vec![Value::text("Birch Row"), bob_in.clone()],
                Some(&bob_in),
            )
            .await,
        );
        let sweep = value(
            call(
                &program,
                &host,
                "add_chore",
                vec![field(&ash, "id"), field(&ann, "id"), Value::text("sweep")],
                Some(&ann_in),
            )
            .await,
        );
        let mow = value(
            call(
                &program,
                &host,
                "add_chore",
                vec![field(&birch, "id"), field(&bob, "id"), Value::text("mow")],
                Some(&bob_in),
            )
            .await,
        );

        // Reading: an owner reads their own; the system, granted read,
        // reads every household's on a schedule's run, and only its
        // tenant's on a subscriber's delivery; no one signed in, nothing.
        let seen = |outcome: Outcome| titles(value(outcome));
        assert_eq!(
            seen(call(&program, &host, "seen_chores", vec![], Some(&ann_in)).await),
            ["sweep"]
        );
        assert_eq!(
            seen(call(&program, &host, "seen_chores", vec![], None).await),
            Vec::<String>::new()
        );
        assert_eq!(
            seen(as_system(&program, &host, "seen_chores", vec![], None).await),
            ["mow", "sweep"]
        );
        assert_eq!(
            seen(as_system(&program, &host, "seen_chores", vec![], Some(&birch)).await),
            ["mow"]
        );

        // Changing: only the system is granted `update`, so the chore's
        // owner is refused and the system isn't, within its tenant.
        let owner = call(
            &program,
            &host,
            "mark_done",
            vec![field(&sweep, "id")],
            Some(&ann_in),
        )
        .await;
        assert!(matches!(owner, Outcome::Failed(_)), "{owner:?}");
        let outside = as_system(
            &program,
            &host,
            "mark_done",
            vec![field(&sweep, "id")],
            Some(&birch),
        )
        .await;
        assert!(matches!(outside, Outcome::Failed(_)), "{outside:?}");
        let done = value(
            as_system(
                &program,
                &host,
                "mark_done",
                vec![field(&sweep, "id")],
                Some(&ash),
            )
            .await,
        );
        assert_eq!(field(&done, "done"), Value::Bool(true));
        let done =
            value(as_system(&program, &host, "mark_done", vec![field(&mow, "id")], None).await);
        assert_eq!(field(&done, "done"), Value::Bool(true));

        // Deleting: only the owner is granted `delete`, so the system is
        // refused, and the chore stays.
        let refused = as_system(&program, &host, "drop_chore", vec![field(&mow, "id")], None).await;
        assert!(matches!(refused, Outcome::Failed(_)), "{refused:?}");
        assert_eq!(
            seen(as_system(&program, &host, "seen_chores", vec![], None).await),
            ["mow", "sweep"]
        );

        // An action listed for `Access.system` alone: the system's to run,
        // and no signed-in caller's.
        let tidied = value(as_system(&program, &host, "tidy", vec![], None).await);
        assert_eq!(field(&tidied, "ok"), Value::Bool(true));
        let refused = call(&program, &host, "tidy", vec![], Some(&ann_in)).await;
        assert_eq!(code(&refused), "not_permitted");

        // A listed action's line decides its own body's delete wherever
        // it is called from (chunk 6.6b): the system deletes through
        // `settle_chore`, which isn't listed, and the owner, whom the
        // chore's own `delete:` names, is refused by `close_chore`'s line.
        let refused = call(
            &program,
            &host,
            "settle_chore",
            vec![field(&sweep, "id")],
            Some(&ann_in),
        )
        .await;
        assert_eq!(code(&refused), "not_permitted");
        let settled = as_system(
            &program,
            &host,
            "settle_chore",
            vec![field(&mow, "id")],
            Some(&birch),
        )
        .await;
        assert!(matches!(settled, Outcome::Value(_)), "{settled:?}");
        assert_eq!(
            seen(as_system(&program, &host, "seen_chores", vec![], None).await),
            ["sweep"]
        );

        // Creating: the system isn't granted `create`, and holds no
        // membership, so it adds nothing.
        let created = as_system(
            &program,
            &host,
            "add_chore",
            vec![field(&ash, "id"), field(&ann, "id"), Value::text("dust")],
            Some(&ash),
        )
        .await;
        assert!(matches!(created, Outcome::Failed(_)), "{created:?}");
    });
}
