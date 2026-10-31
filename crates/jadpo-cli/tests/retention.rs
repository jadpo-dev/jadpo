//! Retention against live PostgreSQL (area 15, "Retention"; area 12, "The
//! purge schedule"; chunk P2), on the interpreter and on the compiled
//! executor, which runs from the plan's canonical bytes as generated code
//! holds them:
//!
//! - a visitor's referrer, whose field has `retention:`, is cleared at the
//!   first hourly run of `Visitor.purge` after its time, and the record
//!   counts as changed;
//! - an invoice, whose entity has `retention:`, is deleted at the first
//!   run after its time, with the lines that cascade from it;
//! - a member who signs in is deleted with their sessions;
//! - what isn't due yet stays, and each run's record counts what it
//!   removed.
//!
//! Then, through the runtime's entry point, an entity's retention and its
//! field's share one run's batches, and a run stops at its budget.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::schedules::{Ran, tick};
use jadpo_runtime::{Executor, Outcome, SqlValue};
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

const POLICY: &str = "    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
";

fn files() -> Vec<(&'static str, String)> {
    vec![
        ("app.jadpo", "use club/entities { Member }\n\napplication {}\n\nprincipal {\n    user: {\n        subject: Member.handle\n    }\n}\n\nauthentication {\n    api_token: {}\n}\n".to_string()),
        (
            "config.jadpo",
            "config {\n    invoice_retention: Duration {\n        default: 60d\n    }\n    member_retention: Duration {\n        default: 90d\n    }\n    referrer_retention: Duration {\n        default: 30d\n    }\n    visitor_retention: Duration {\n        default: 365d\n    }\n}\n".to_string(),
        ),
        (
            "domains/club/types/name.jadpo",
            "/// A name.\ntype Name = Text {\n    max_length: 80\n}\n".to_string(),
        ),
        (
            "domains/club/types/referrer.jadpo",
            "/// Where a visitor came from.\ntype Referrer = Text {\n    max_length: 200\n}\n".to_string(),
        ),
        (
            "domains/club/entities/visitor.jadpo",
            format!(
                "use club/types {{ Name, Referrer }}

/// Someone who looked round, whose referrer is kept for 30 days.
entity Visitor {{
    id: Uuid
    name: Name
    referrer: Referrer? {{
        personal: PersonalData.online
        retention: {{
            after: config.referrer_retention
            since: created_at
        }}
    }}
    created_at: Instant {{
        generated: Generated.on_create
    }}
    changed_at: Instant {{
        generated: Generated.on_change
    }}

    retention: {{
        after: config.visitor_retention
        since: created_at
    }}
    person: true

{POLICY}}}
"
            ),
        ),
        (
            "domains/club/entities/invoice.jadpo",
            format!(
                "use club/types {{ Name }}

/// An invoice, kept for 60 days after it was issued.
entity Invoice {{
    id: Uuid
    number: Name
    issued_at: Instant {{
        generated: Generated.on_create
    }}

    retention: {{
        after: config.invoice_retention
        since: issued_at
    }}

{POLICY}}}
"
            ),
        ),
        (
            "domains/club/entities/invoice_line.jadpo",
            format!(
                "use club/entities {{ Invoice }}
use club/types {{ Name }}

/// A line of an invoice, which goes with it.
entity InvoiceLine {{
    id: Uuid
    invoice_id: Invoice.id {{
        on_delete: OnDelete.cascade
    }}
    note: Name

{POLICY}}}
"
            ),
        ),
        (
            "domains/club/entities/member.jadpo",
            format!(
                "use club/types {{ Name }}

/// A member who signs in, kept for 90 days after they joined.
entity Member {{
    id: Uuid
    handle: Name {{
        unique: true
    }}
    joined_at: Instant {{
        generated: Generated.on_create
    }}

    retention: {{
        after: config.member_retention
        since: joined_at
    }}

{POLICY}}}
"
            ),
        ),
        (
            "domains/club/actions/visit.jadpo",
            "use club/entities { Visitor }\n\n/// Records a visitor.\naction visit(name: Visitor.name, referrer: Visitor.referrer) -> Visitor {\n    return Visitor.create {\n        set: {\n            name: name\n            referrer: referrer\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/club/actions/issue.jadpo",
            "use club/entities { Invoice }\n\n/// Issues an invoice.\naction issue(number: Invoice.number) -> Invoice {\n    return Invoice.create {\n        set: {\n            number: number\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/club/actions/add_line.jadpo",
            "use club/entities { Invoice, InvoiceLine }\n\n/// Adds a line to an invoice.\naction add_line(invoice: Invoice, note: InvoiceLine.note) -> InvoiceLine {\n    return InvoiceLine.create {\n        set: {\n            invoice_id: invoice.id\n            note: note\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/club/actions/join.jadpo",
            "use club/entities { Member }\nuse club/failures { HandleTaken }\n\n/// Adds a member.\naction join(handle: Member.handle) fails HandleTaken -> Member {\n    return attempt Member.create {\n        set: {\n            handle: handle\n        }\n        conflict: {\n            Member.handle: HandleTaken\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/club/failures/handle_taken.jadpo",
            "/// Someone has that handle already.\ntype HandleTaken = Failure {\n    kind: FailureKind.conflict\n}\n".to_string(),
        ),
    ]
}

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("retention-p2-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files() {
        write(&root, path, &text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

/// 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;

/// Runs `action` with `args` at the harness's clock, as no one.
async fn call(program: &Program, host: &Harness, action: &str, args: Vec<Value>) -> Value {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program);
    match program.run(&mut ctx, id, args).await {
        Outcome::Value(value) => value,
        other => panic!("{action}: expected a value, got {other:?}"),
    }
}

/// The text in each row's first column, `none` for `NULL`.
async fn texts(host: &Harness, sql: &str) -> Vec<String> {
    host.rows(sql)
        .await
        .into_iter()
        .map(|row| match &row[0] {
            SqlValue::Text(text) => text.clone(),
            SqlValue::Null(_) => "none".to_string(),
            other => panic!("not text: {other:?}"),
        })
        .collect()
}

/// Each visitor's name and referrer, by name.
async fn referrers(host: &Harness) -> Vec<String> {
    texts(
        host,
        "SELECT \"name\" || ' ' || coalesce(\"referrer\", 'none') FROM \"visitor\" ORDER BY \"name\"",
    )
    .await
}

/// The record each run of `schedule` counted, oldest first.
async fn counted(host: &Harness, schedule: &str) -> Vec<Option<i32>> {
    host.rows(&format!(
        "SELECT \"items\" FROM \"jadpo_schedule_runs\" WHERE \"schedule\" = 'schedule {schedule}' AND \"outcome\" = 'succeeded' ORDER BY \"started_at\""
    ))
    .await
    .into_iter()
    .map(|row| match &row[0] {
        SqlValue::Int4(items) => Some(*items),
        _ => None,
    })
    .collect()
}

/// The compiled executor on `program`'s plan, as generated code holds it:
/// its canonical bytes, decoded as the program starts. The purge
/// schedule's run calls no callable, so none is given.
fn compiled(program: &Program) -> jadpo_runtime::compiled::Program {
    use jadpo_runtime::compiled::{Code, Shared};
    let bytes = program
        .plan()
        .runtime()
        .encode_deployable()
        .expect("the plan's bytes");
    let code: &'static Code = Box::leak(Box::new(Code {
        plan: Box::leak(bytes.into_boxed_slice()),
        places: &[],
        callable: |_| None,
        row: |_, _| None,
        routes: |_| None,
        shared: Box::leak(Box::new(Shared::new())),
    }));
    jadpo_runtime::compiled::Program::new(code).expect("the plan decodes")
}

/// The whole story: records made through the interpreter, then the
/// hourly ticks run by the interpreter, or by the compiled executor when
/// `compiled_too`, each value and record gone at the first run after its
/// time.
fn values_and_records_go_at_the_first_run_after_their_time(name: &str, compiled_too: bool) {
    let root = project(name);
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let made = compiled_too.then(|| compiled(&program));
    let ticking: &dyn Executor = match &made {
        Some(made) => made,
        None => &program,
    };
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url).at(T0).random_from(0);
    let runtime = Runtime::new().expect("runtime");
    let tick_at = |at: i64| {
        host.set_now(at);
        runtime.block_on(tick(ticking, &host, at))
    };
    let ran = |ticked: &jadpo_runtime::schedules::Ticked, schedule: &str| -> Vec<Ran> {
        ticked
            .due
            .iter()
            .filter(|due| due.schedule == format!("schedule {schedule}"))
            .map(|due| due.ran.clone())
            .collect()
    };
    // What each purge's run counted, values cleared and records deleted
    // (an invoice's lines go with it, uncounted): the visitors', the
    // invoices' and the members'.
    let counts = |ticked: &jadpo_runtime::schedules::Ticked| -> [Option<u64>; 3] {
        ["Visitor.purge", "Invoice.purge", "Member.purge"].map(|schedule| {
            ticked
                .due
                .iter()
                .find(|due| due.schedule == format!("schedule {schedule}"))
                .and_then(|due| due.items)
        })
    };

    // Ada visits, an invoice with two lines is issued and Ada joins at T0;
    // Bob does the same ten days later.
    runtime.block_on(async {
        for (at, who, number) in [(T0, "Ada", "A-1"), (T0 + 10 * DAY, "Bob", "A-2")] {
            host.set_now(at);
            call(
                &program,
                &host,
                "visit",
                vec![Value::text(who), Value::text(&format!("{who}'s search"))],
            )
            .await;
            let invoice = call(&program, &host, "issue", vec![Value::text(number)]).await;
            for line in ["tea", "cake"] {
                call(
                    &program,
                    &host,
                    "add_line",
                    vec![invoice.clone(), Value::text(&format!("{number} {line}"))],
                )
                .await;
            }
            call(&program, &host, "join", vec![Value::text(&who.to_lowercase())]).await;
        }
        // Each member holds an API token's session, ending long after.
        host.rows(
            "INSERT INTO \"jadpo_sessions\" (\"id\", \"principal_id\", \"verifier\", \"started_at\", \"ends_at\") SELECT gen_random_uuid(), \"id\", 'verifier of ' || \"handle\", TIMESTAMPTZ '2026-10-19 09:00:00+00', TIMESTAMPTZ '2030-01-01 00:00:00+00' FROM \"member\"",
        )
        .await;
    });

    // The first tick keeps the schedules' state; an hour on, each purge
    // runs and finds nothing due.
    let first = tick_at(T0 + 10 * DAY);
    assert!(ran(&first, "Visitor.purge").is_empty(), "{:?}", first.due);
    let nothing = tick_at(T0 + 10 * DAY + HOUR);
    for schedule in ["Visitor.purge", "Invoice.purge", "Member.purge"] {
        assert_eq!(
            ran(&nothing, schedule),
            [Ran::Succeeded { events: 0 }],
            "{schedule}"
        );
    }
    assert_eq!(counts(&nothing), [Some(0), Some(0), Some(0)]);

    // Thirty days to the second after Ada visited, her referrer stays: a
    // value goes only once it is older than its period.
    let boundary = tick_at(T0 + 30 * DAY);
    assert_eq!(counts(&boundary), [Some(0), Some(0), Some(0)]);
    runtime.block_on(async {
        assert_eq!(
            referrers(&host).await,
            ["Ada Ada's search", "Bob Bob's search"]
        );
    });

    // The next hourly run clears it, and the record counts as changed;
    // Bob's, ten days younger, stays.
    let cleared = tick_at(T0 + 30 * DAY + HOUR);
    assert_eq!(
        ran(&cleared, "Visitor.purge"),
        [Ran::Succeeded { events: 0 }]
    );
    assert_eq!(counts(&cleared), [Some(1), Some(0), Some(0)]);
    runtime.block_on(async {
        assert_eq!(referrers(&host).await, ["Ada none", "Bob Bob's search"]);
        let changed = texts(
            &host,
            "SELECT to_char(\"changed_at\" AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI') FROM \"visitor\" ORDER BY \"name\"",
        )
        .await;
        assert_eq!(changed, ["2026-11-18 10:00", "2026-10-29 09:00"]);
    });

    // Sixty days and an hour after A-1 was issued, it goes with its lines,
    // and Bob's referrer is past its time too.
    let invoiced = tick_at(T0 + 60 * DAY + HOUR);
    assert_eq!(counts(&invoiced), [Some(1), Some(1), Some(0)]);
    runtime.block_on(async {
        assert_eq!(
            texts(
                &host,
                "SELECT \"number\" FROM \"invoice\" ORDER BY \"number\""
            )
            .await,
            ["A-2"]
        );
        assert_eq!(
            texts(
                &host,
                "SELECT \"note\" FROM \"invoice_line\" ORDER BY \"note\""
            )
            .await,
            ["A-2 cake", "A-2 tea"]
        );
        assert_eq!(referrers(&host).await, ["Ada none", "Bob none"]);
    });

    // Ninety days and an hour after Ada joined, she goes with her session;
    // Bob, who joined ten days later, keeps his. A-2 has gone too.
    let joined = tick_at(T0 + 90 * DAY + HOUR);
    assert_eq!(counts(&joined), [Some(0), Some(1), Some(1)]);
    runtime.block_on(async {
        assert_eq!(
            texts(
                &host,
                "SELECT \"handle\" FROM \"member\" ORDER BY \"handle\""
            )
            .await,
            ["bob"]
        );
        assert_eq!(
            texts(
                &host,
                "SELECT \"verifier\" FROM \"jadpo_sessions\" ORDER BY \"verifier\""
            )
            .await,
            ["verifier of bob"]
        );
        assert!(
            texts(&host, "SELECT \"number\" FROM \"invoice\"")
                .await
                .is_empty()
        );
        // The visitors themselves are kept for a year.
        assert_eq!(referrers(&host).await, ["Ada none", "Bob none"]);
        // The run records count the same; those over 30 days old have
        // gone, as every schedule's do.
        assert_eq!(counted(&host, "Member.purge").await, [Some(1)]);
        assert_eq!(counted(&host, "Visitor.purge").await, [Some(0)]);
    });
    drop(host);
    databases.release(copy, false);
}

#[test]
fn retention_on_the_interpreter() {
    if !jadpo_test::live() {
        return;
    }
    values_and_records_go_at_the_first_run_after_their_time("interpreted", false);
}

#[test]
fn retention_compiled() {
    if !jadpo_test::live() {
        return;
    }
    values_and_records_go_at_the_first_run_after_their_time("compiled", true);
}

#[test]
fn an_entitys_retention_and_its_fields_share_a_runs_batches() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("batches");
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let started = program.started();
    let plans = program.runtime_plan().purges_of("Visitor", started);
    let kinds: Vec<String> = plans.iter().map(|(plan, _)| plan.what()).collect();
    assert_eq!(
        kinds,
        [
            "the retention of `Visitor`",
            "the retention of `Visitor.referrer`"
        ],
        "the whole record first, then the field"
    );
    assert_eq!(plans[0].1, Some(365 * DAY));
    assert_eq!(plans[1].1, Some(30 * DAY));
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        // 700 visitors from long ago, with referrers, and 300 from a
        // fortnight before the run, whose referrers are due but who stay.
        host.rows(
            "INSERT INTO \"visitor\" (\"id\", \"name\", \"referrer\", \"created_at\", \"changed_at\") \
             SELECT gen_random_uuid(), 'old ' || n, 'search', TIMESTAMPTZ '1999-01-01 00:00:00+00', TIMESTAMPTZ '1999-01-01 00:00:00+00' FROM generate_series(1, 700) AS n \
             UNION ALL \
             SELECT gen_random_uuid(), 'recent ' || n, 'search', TIMESTAMPTZ '2026-08-01 00:00:00+00', TIMESTAMPTZ '2026-08-01 00:00:00+00' FROM generate_series(1, 300) AS n",
        )
        .await;
        let at = T0;
        // A run stops at its budget; the next carries on.
        let first = jadpo_runtime::purge_all(&host, &plans, at, 1)
            .await
            .expect("purge");
        assert_eq!(first.batches, vec![500]);
        assert!(!first.finished, "stopped at its budget");
        let rest = jadpo_runtime::purge_all(&host, &plans, at, jadpo_runtime::PURGE_BATCHES)
            .await
            .expect("purge");
        assert_eq!(rest.batches, vec![200, 300], "the last old ones, then the referrers");
        assert!(rest.finished);
        let left = texts(
            &host,
            "SELECT count(*)::text || ' ' || count(\"referrer\")::text FROM \"visitor\"",
        )
        .await;
        assert_eq!(left, ["300 0"]);
    });
    drop(host);
    databases.release(copy, false);
}
