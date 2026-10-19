//! The built-in purge schedule and schedule tests (area 5, "Lifecycle";
//! area 12, "Schedules" and "Watching schedules"; area 13, "Kinds of
//! test"; chunk 6.2c).
//!
//! A club whose members are purged 30 days after they leave, and a
//! `headcount` schedule counting the members every hour:
//!
//! - `Member`'s lifecycle has `purge:`, so the compiler gives it an hourly
//!   schedule, `Member.purge`: its state and run records are in the
//!   migration snapshot, the tick runs it like any schedule, deleting the
//!   members hidden for 30 days, its run record counts them, and
//!   `jadpo schedules` lists it beside `headcount`;
//! - `headcount.run()` in a test runs the schedule once at the test's clock,
//!   and `jadpo test --kind schedule` runs only such tests.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_interp::Program;
use jadpo_runtime::schedules::{Ran, tick};
use jadpo_runtime::{Clock, Ctx, Database, Env, Executor, Host, Outcome, Row, SqlValue};
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
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = crate::support::without_run_time(&output.stdout);
    output
}

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/club/types/name.jadpo",
        "/// A name.\ntype Name = Text {\n    max_length: 80\n}\n",
    ),
    (
        "domains/club/types/count.jadpo",
        "/// How many.\ntype Count = Int {\n    min: 0\n}\n",
    ),
    (
        "domains/club/types/taken_at.jadpo",
        "/// When a tally was taken.\ntype TakenAt = Instant {}\n",
    ),
    (
        "domains/club/entities/member.jadpo",
        "use club/types { Name }

/// Someone in the club.
entity Member {
    id: Uuid
    name: Name
    left_at: Instant?

    lifecycle: {
        visible: left_at == none
        transitions: {
            delete: {
                when: left_at == none
                set: {
                    left_at: clock.now
                }
            }
        }
        purge: {
            after: 30d
            since: left_at
        }
    }

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.public]
        delete: [Access.public]
    }
}
",
    ),
    (
        "domains/club/entities/tally.jadpo",
        "use club/types { Count, TakenAt }

/// How many members there were at a time.
entity Tally {
    id: Uuid
    seen: Count
    taken_at: TakenAt

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
",
    ),
    (
        "domains/club/types/member_view.jadpo",
        "use club/entities { Member }\n\n/// A member, briefly.\ntype MemberView = Output {\n    pick: [Member.id, Member.name]\n}\n",
    ),
    (
        "domains/club/types/tally_view.jadpo",
        "use club/entities { Tally }\n\n/// A tally, briefly.\ntype TallyView = Output {\n    pick: [Tally.seen, Tally.taken_at]\n}\n",
    ),
    (
        "domains/club/failures/member_not_found.jadpo",
        "/// No member has that id.\ntype MemberNotFound = Failure {\n    kind: FailureKind.not_found\n}\n",
    ),
    (
        "domains/club/actions/join.jadpo",
        "use club/entities { Member }\n\n/// Adds a member.\naction join(name: Member.name) -> Member {\n    return Member.create {\n        set: {\n            name: name\n        }\n    }\n}\n",
    ),
    (
        "domains/club/actions/leave.jadpo",
        "use club/entities { Member }\nuse club/failures { MemberNotFound }\n\n/// A member leaves.\naction leave(member_id: Member.id) fails MemberNotFound -> Unit {\n    attempt Member.delete(member_id)\n}\n",
    ),
    (
        "domains/club/actions/take_tally.jadpo",
        "use club/entities { Tally }
use club/queries { members }
use club/types { Count, MemberView, TakenAt }

/// Counts the members.
action take_tally() -> Unit {
    var seen: List<MemberView> = members()
    var tally: Tally = Tally.create {
        set: {
            seen: Count(list.count(items: seen))
            taken_at: TakenAt(clock.now)
        }
    }
    assert tally.seen >= 0
}
",
    ),
    (
        "domains/club/queries/members.jadpo",
        "use club/entities { Member }\nuse club/types { MemberView }\n\n/// The members.\nquery members() -> List<MemberView> {\n    from: Member\n    order: [Member.name asc]\n    limit: 50\n}\n",
    ),
    (
        "domains/club/queries/tallies.jadpo",
        "use club/entities { Tally }\nuse club/types { TallyView }\n\n/// The tallies, oldest first.\nquery tallies() -> List<TallyView> {\n    from: Tally\n    order: [Tally.taken_at asc]\n    limit: 50\n}\n",
    ),
    (
        "domains/club/schedules/headcount.jadpo",
        "use club/actions { take_tally }\n\n/// Counts the members every hour.\nschedule headcount {\n    every: 1h\n    run: take_tally()\n}\n",
    ),
];

/// A schedule test: the run reads the test's clock, and sees what the
/// test set up before it.
const SCHEDULE_TESTS: &str = "use club/entities { Member }
use club/actions { join, leave }
use club/queries { tallies }

test \"a run counts the members at the test's clock\" {
    clock: \"2026-01-15T12:00:00Z\"
    var ada: Member = join(\"Ada\")
    var bob: Member = join(\"Bob\")
    assert ada.left_at == none
    attempt leave(bob.id)
    headcount.run()
    clock.advance(1h)
    headcount.run()
    assert list.map(tallies(), t -> t.seen) == [1, 1]
    assert list.map(tallies(), t -> t.taken_at) == [\"2026-01-15T12:00:00Z\", \"2026-01-15T13:00:00Z\"]
}
";

/// An action test beside it, which `--kind schedule` leaves out.
const ACTION_TESTS: &str = "use club/entities { Member }

test \"joining adds a member\" {
    var ada: Member = join(\"Ada\")
    assert ada.name == \"Ada\"
}
";

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("purge-schedule-6.2c-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    write(
        &root,
        "domains/club/schedules/headcount.test.jadpo",
        SCHEDULE_TESTS,
    );
    write(&root, "domains/club/actions/join.test.jadpo", ACTION_TESTS);
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
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
    async fn rows(&self, sql: &str) -> Vec<Row> {
        self.db
            .query_once(sql, &[])
            .await
            .expect("the store answers")
    }

    async fn names(&self) -> Vec<String> {
        self.rows("SELECT \"name\" FROM \"member\" ORDER BY \"name\"")
            .await
            .into_iter()
            .map(|row| match &row[0] {
                SqlValue::Text(text) => text.clone(),
                other => panic!("not text: {other:?}"),
            })
            .collect()
    }

    /// Each purge run record's outcome and the records it counted, oldest
    /// first.
    async fn purge_runs(&self) -> Vec<(String, Option<i32>)> {
        self.rows("SELECT \"outcome\", \"items\" FROM \"jadpo_schedule_runs\" WHERE \"schedule\" = 'schedule Member.purge' ORDER BY \"started_at\"")
            .await
            .into_iter()
            .map(|row| match row.as_slice() {
                [SqlValue::Text(outcome), items] => (
                    outcome.clone(),
                    match items {
                        SqlValue::Int4(items) => Some(*items),
                        _ => None,
                    },
                ),
                other => panic!("not a run: {other:?}"),
            })
            .collect()
    }
}

/// Runs `action` with `args` at the store's clock, as no one.
async fn call(program: &Program, host: &Store, action: &str, args: Vec<Value>) -> Value {
    let id = program.plan().find(action).expect("the action");
    host.seeds.set(host.seeds.get() + 1);
    let env = Env {
        seed: Some(host.seeds.get()),
        clock: Clock::fixed(host.clock.get()),
    };
    let mut ctx = Ctx::new(program.places(), host, env);
    match program.run(&mut ctx, id, args).await {
        Outcome::Value(value) => value,
        other => panic!("{action}: expected a value, got {other:?}"),
    }
}

fn id_of(value: &Value) -> Value {
    match value {
        Value::Record(record) => record.get("id").cloned().expect("an id"),
        other => panic!("not a record: {other:?}"),
    }
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
    output
}

/// 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;

#[test]
fn the_purge_schedule_is_in_the_snapshot_and_listed_with_the_rest() {
    let root = project("snapshot");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let snapshot = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("snapshot.json"))
        .find(|path| path.exists())
        .expect("a snapshot");
    let snapshot = fs::read_to_string(snapshot).expect("snapshot");
    for table in ["jadpo_schedules", "jadpo_schedule_runs"] {
        assert!(snapshot.contains(table), "{table} is in the snapshot");
    }
    assert!(snapshot.contains("\"items\""), "a run counts its items");

    // Without the `headcount` schedule, the purge still keeps its tables.
    let only = Path::new(env!("CARGO_TARGET_TMPDIR")).join("purge-schedule-6.2c-only");
    let _ = fs::remove_dir_all(&only);
    for (path, text) in FILES.iter().filter(|(path, _)| {
        !["tall", "count", "taken_at", "headcount"]
            .iter()
            .any(|word| path.contains(word))
    }) {
        write(&only, path, text);
    }
    let migrated = jadpo(&only, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let loaded = jadpo_db::load(&only);
    let schema = jadpo_check::data::schema(&loaded.db, loaded.project);
    assert!(
        schema.stores["main"].tables.contains_key("jadpo_schedules"),
        "a purge is a schedule"
    );
    let plan = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let schedules = plan.plan().schedules();
    let keys: Vec<(&str, &str, u64)> = schedules
        .schedules
        .iter()
        .map(|schedule| {
            (
                schedule.key.as_str(),
                schedule.name.as_str(),
                schedule.every_ms,
            )
        })
        .collect();
    assert_eq!(
        keys,
        [("schedule Member.purge", "Member.purge", HOUR as u64)]
    );
}

#[test]
fn the_purge_runs_hourly_on_the_tick_and_is_recorded_like_any_schedule() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("ticks");
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Store {
        db: postgres(&url).expect("store"),
        seeds: Cell::new(0),
        clock: Cell::new(T0),
        drawn: Cell::new(0),
    };
    let runtime = Runtime::new().expect("runtime");
    let tick_at = |at: i64| {
        host.clock.set(at);
        runtime.block_on(tick(&program, &host, at))
    };
    let purges = |ticked: &jadpo_runtime::schedules::Ticked| -> Vec<Ran> {
        ticked
            .due
            .iter()
            .filter(|due| due.schedule == "schedule Member.purge")
            .map(|due| due.ran.clone())
            .collect()
    };

    // Ada stays; Bob and Cy leave at T0, Dee a day later.
    let (ada, bob, cy, dee) = runtime.block_on(async {
        let ada = call(&program, &host, "join", vec![Value::text("Ada")]).await;
        let bob = call(&program, &host, "join", vec![Value::text("Bob")]).await;
        let cy = call(&program, &host, "join", vec![Value::text("Cy")]).await;
        let dee = call(&program, &host, "join", vec![Value::text("Dee")]).await;
        for member in [&bob, &cy] {
            call(&program, &host, "leave", vec![id_of(member)]).await;
        }
        host.clock.set(T0 + DAY);
        call(&program, &host, "leave", vec![id_of(&dee)]).await;
        (ada, bob, cy, dee)
    });
    let _ = (ada, bob, cy, dee);

    // The first tick keeps its state; the purge is first due an hour on.
    let first = tick_at(T0 + DAY);
    assert!(purges(&first).is_empty(), "{:?}", first.due);
    let early = tick_at(T0 + DAY + 30 * 60_000);
    assert!(purges(&early).is_empty(), "not due yet");

    // An hour on it runs: nobody has been hidden for 30 days yet.
    let ran = tick_at(T0 + DAY + HOUR);
    assert_eq!(purges(&ran), [Ran::Succeeded { events: 0 }]);
    let due = ran
        .due
        .iter()
        .find(|due| due.schedule == "schedule Member.purge")
        .expect("the purge ran");
    assert_eq!(due.items, Some(0));
    runtime.block_on(async {
        assert_eq!(host.names().await, ["Ada", "Bob", "Cy", "Dee"]);
    });

    // Thirty days and an hour after Bob and Cy left, the hourly run takes
    // them; Dee, who left a day later, stays.
    let later = T0 + 30 * DAY + HOUR;
    let ran = tick_at(later);
    assert_eq!(purges(&ran), [Ran::Succeeded { events: 0 }]);
    runtime.block_on(async {
        assert_eq!(host.names().await, ["Ada", "Dee"]);
        assert_eq!(
            host.purge_runs().await,
            [
                ("succeeded".to_string(), Some(0)),
                ("succeeded".to_string(), Some(2))
            ]
        );
    });

    // `jadpo schedules` lists it beside `headcount`, with what its last run
    // purged; `jadpo why` points at its entity's `purge:`.
    let listed = watching(&root, &url, later + 20_000, &["schedules"]);
    assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
    assert!(
        listed
            .stdout
            .contains("Member.purge  ✓ succeeded  10:00:00 · 0 ms · 2 records purged"),
        "{}",
        listed.stdout
    );
    assert!(listed.stdout.contains("headcount"), "{}", listed.stdout);
    let why = watching(&root, &url, later + 20_000, &["why", "Member.purge"]);
    assert_eq!(why.code, 0, "{}{}", why.stdout, why.stderr);
    assert!(
        why.stdout
            .contains("schedule  domains/club/entities/member.jadpo:19"),
        "{}",
        why.stdout
    );
    let history = watching(
        &root,
        &url,
        later + 20_000,
        &["schedules", "history", "Member.purge"],
    );
    assert_eq!(history.code, 0, "{}{}", history.stdout, history.stderr);
    assert!(
        history.stdout.contains("2 records purged"),
        "{}",
        history.stdout
    );
    // An operator runs it now, under its lease, as any schedule.
    let ran = watching(
        &root,
        &url,
        later + 60_000,
        &["schedules", "run", "Member.purge"],
    );
    assert_eq!(ran.code, 0, "{}{}", ran.stdout, ran.stderr);
    assert!(ran.stdout.contains("0 records purged"), "{}", ran.stdout);

    drop(host);
    databases.release(copy, false);
}

#[test]
fn a_schedule_test_runs_the_schedule_at_its_clock_and_has_its_own_kind() {
    let root = project("tests");
    if !jadpo_test::live() {
        return;
    }
    let all = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(all.code, 0, "{}{}", all.stdout, all.stderr);
    assert!(all.stdout.contains("2 passed"), "{}", all.stdout);

    let scheduled = jadpo(&root, &["test", "--kind", "schedule", "--seed", "1"]);
    assert_eq!(
        scheduled.code, 0,
        "{}{}",
        scheduled.stdout, scheduled.stderr
    );
    assert_eq!(scheduled.stdout, "tested: 1 passed, 1 of 2 tests\n");

    let document: serde_json::Value = serde_json::from_str(
        &jadpo(
            &root,
            &["test", "--kind", "schedule", "--json", "--seed", "1"],
        )
        .stdout,
    )
    .expect("JSON");
    assert_eq!(document["ok"], true, "{document}");
}
