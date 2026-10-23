//! The tick's remaining deletes against live PostgreSQL (area 12, "The
//! tick"; area 8; chunk 6.10): ended browser sessions, API tokens and
//! expired service keys are deleted in the store each is in, as a lookup
//! already ignores them, while live ones stay; and a subscriber's handled
//! records go once their delivery can no longer repeat, which is once its
//! event, and so its delivery record, is gone: in the event's own store
//! with the event, in one statement, and in any other store after it.
//! Each delete takes 1,000 rows at a time, ten times a tick.
//!
//! What an operator settled (chunk 6.13): a delivery dismissed or
//! cancelled goes 30 days after the operator's last action on it, with
//! its handled records, and its event once no delivery of it is left that
//! could run; the operator's log rows go 30 days after they were written,
//! once their delivery is gone. A delivery that could still run stays.
//!
//! A help desk with two stores: members sign in with browser sessions and
//! API tokens, kept beside them in `main`; partners with service keys,
//! kept beside them in `tallies`. Adding a note emits `NoteAdded`, which
//! `mark_note_added` marks in `main` and `tally_note_added` tallies in
//! `tallies`, each committing its handled record where it writes.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_interp::Program;
use jadpo_runtime::deliveries::{Action, Done, Holds, act_on_delivery, find};
use jadpo_runtime::schedules::tick;
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
    run(&args, &env.with_cwd(cwd))
}

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use desk/entities { Member, Partner }

application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        tallies: {
            driver: Driver.postgres
        }
    }
}

principal {
    user: {
        subject: Member.handle
    }
    service: {
        subject: Partner.name
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "desk_session"
    }
    api_token: {}
    service_key: {}
}
"#,
    ),
    (
        "domains/desk/actions/add_note.jadpo",
        r#"use desk/entities { Note }
use desk/events { NoteAdded }
use desk/types { Title }

action add_note(body: Title) -> Unit {
    var note: Note = Note.create {
        set: {
            body: body
        }
    }
    emit NoteAdded {
        note_id: note.id
    }
}
"#,
    ),
    (
        "domains/desk/actions/mark_note.jadpo",
        r#"use desk/entities { Mark }

action mark_note() -> Unit {
    var mark: Mark = Mark.create {
        set: {
            seen: true
        }
    }
    assert mark.seen
}
"#,
    ),
    (
        "domains/desk/actions/tally_note.jadpo",
        r#"use desk/entities { Tally }

action tally_note() -> Unit {
    var tally: Tally = Tally.create {
        set: {
            seen: true
        }
    }
    assert tally.seen
}
"#,
    ),
    (
        "domains/desk/entities/mark.jadpo",
        r#"entity Mark {
    id: Uuid
    seen: Bool

    store: main

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
"#,
    ),
    (
        "domains/desk/entities/member.jadpo",
        r#"use desk/types { Handle }

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    store: main

    policy: {
        create: [Access.public]
        read: [id]
    }
}
"#,
    ),
    (
        "domains/desk/entities/note.jadpo",
        r#"use desk/types { Title }

entity Note {
    id: Uuid
    body: Title

    store: main

    policy: {
        create: [Access.public]
        read: [Access.system]
    }
}
"#,
    ),
    (
        "domains/desk/entities/partner.jadpo",
        r#"use desk/types { Handle }

entity Partner {
    id: Uuid
    name: Handle {
        unique: true
    }

    store: tallies

    policy: {
        create: [Access.system]
        read: [id]
    }
}
"#,
    ),
    (
        "domains/desk/entities/tally.jadpo",
        r#"entity Tally {
    id: Uuid
    seen: Bool

    store: tallies

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
"#,
    ),
    (
        "domains/desk/events/note_added.jadpo",
        r#"use desk/entities { Note }

/// A note was added to the desk.
type NoteAdded = Event {
    note_id: Note.id
}
"#,
    ),
    (
        "domains/desk/subscribers/mark_note_added.jadpo",
        r#"use desk/actions { mark_note }
use desk/events { NoteAdded }

/// Marks each note added, in the store the note is in.
subscriber mark_note_added {
    on: NoteAdded
    run: mark_note()
}
"#,
    ),
    (
        "domains/desk/subscribers/tally_note_added.jadpo",
        r#"use desk/actions { tally_note }
use desk/events { NoteAdded }

/// Tallies each note added, in the other store.
subscriber tally_note_added {
    on: NoteAdded
    run: tally_note()
}
"#,
    ),
    (
        "domains/desk/types/handle.jadpo",
        r#"type Handle = Text {
    min_length: 3
    max_length: 40
}
"#,
    ),
    (
        "domains/desk/types/title.jadpo",
        r#"type Title = Text {
    min_length: 1
    max_length: 80
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("tick-deletes-6.10-{name}"));
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

/// Both stores, a clock the test moves, and random bytes from a counter.
struct Stores {
    main: Postgres,
    tallies: Postgres,
    seeds: Cell<u64>,
    clock: Cell<i64>,
    drawn: Cell<u64>,
}

impl Host for Stores {
    fn database(&self, store: &str) -> Option<&dyn Database> {
        match store {
            "main" => Some(&self.main),
            "tallies" => Some(&self.tallies),
            _ => None,
        }
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

impl Stores {
    fn ctx<'p>(&'p self, program: &'p Program) -> Ctx<'p> {
        self.seeds.set(self.seeds.get() + 1);
        let env = Env {
            seed: Some(self.seeds.get()),
            clock: Clock::fixed(self.clock.get()),
        };
        Ctx::of(program, self, env)
    }

    async fn rows(&self, store: &Postgres, sql: &str) -> Vec<Row> {
        store.query_once(sql, &[]).await.expect("the store answers")
    }

    async fn count(&self, store: &Postgres, sql: &str) -> i64 {
        match self
            .rows(store, sql)
            .await
            .first()
            .and_then(|row| row.first())
        {
            Some(SqlValue::Int8(n)) => *n,
            other => panic!("not a count: {other:?}"),
        }
    }

    /// Each store's handled records: `main`'s, then `tallies`'.
    async fn handled(&self) -> (i64, i64) {
        let sql = "SELECT count(*) FROM \"jadpo_handled_deliveries\"";
        (
            self.count(&self.main, sql).await,
            self.count(&self.tallies, sql).await,
        )
    }
}

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

async fn add_note(program: &Program, host: &Stores, text: &str) {
    let id = program.plan().find("add_note").expect("the action");
    let mut ctx = host.ctx(program);
    match program.run(&mut ctx, id, vec![Value::text(text)]).await {
        Outcome::Value(_) => {}
        other => panic!("a note is added: {other:?}"),
    }
}

/// The project checked, migrated into two test databases, and a host on
/// them; `None` when no PostgreSQL is reachable.
fn load(name: &str) -> Option<(Program, jadpo_test::database::TestDatabases, Stores)> {
    let root = project(name);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let plan = program.runtime_plan().schedules().expect("the tick's plan");
    assert_eq!(plan.stores, ["main", "tallies"]);
    assert_eq!(plan.sessions.as_deref(), Some("main"));
    assert_eq!(plan.keys.as_deref(), Some("tallies"));
    if !jadpo_test::live() {
        return None;
    }
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, main) = copy.stores.get("main").expect("main").clone();
    let (_, tallies) = copy.stores.get("tallies").expect("tallies").clone();
    let host = Stores {
        main: postgres(&main).expect("main"),
        tallies: postgres(&tallies).expect("tallies"),
        seeds: Cell::new(0),
        clock: Cell::new(T0),
        drawn: Cell::new(0),
    };
    Some((program, databases, host))
}

#[test]
fn ended_sessions_tokens_and_keys_are_deleted_and_live_ones_kept() {
    let Some((program, _databases, host)) = load("sessions") else {
        return;
    };
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        // In `main`, beside the members: a browser session and an API
        // token (no CSRF verifier) that ended an hour ago, one ending just
        // now, and one of each still live.
        host.rows(&host.main, "INSERT INTO \"jadpo_sessions\" (\"id\", \"principal_id\", \"verifier\", \"csrf_verifier\", \"started_at\", \"ends_at\") VALUES \
            ('00000000-0000-7000-8000-000000000001', '00000000-0000-7000-8000-0000000000a1', 'ended session', 'c1', '2026-09-19T08:00:00Z', '2026-10-19T08:00:00Z'), \
            ('00000000-0000-7000-8000-000000000002', '00000000-0000-7000-8000-0000000000a1', 'ended token', NULL, '2026-09-19T08:00:00Z', '2026-10-19T08:00:00Z'), \
            ('00000000-0000-7000-8000-000000000003', '00000000-0000-7000-8000-0000000000a1', 'ending session', 'c3', '2026-09-19T09:00:00Z', '2026-10-19T09:00:00Z'), \
            ('00000000-0000-7000-8000-000000000004', '00000000-0000-7000-8000-0000000000a1', 'live session', 'c4', '2026-10-19T08:00:00Z', '2026-11-18T08:00:00Z'), \
            ('00000000-0000-7000-8000-000000000005', '00000000-0000-7000-8000-0000000000a1', 'live token', NULL, '2026-10-19T08:00:00Z', '2026-11-18T08:00:00Z') \
            RETURNING 1")
            .await;
        // In `tallies`, beside the partners: a key that expired, and one
        // that hasn't.
        host.rows(&host.tallies, "INSERT INTO \"jadpo_service_keys\" (\"id\", \"principal_id\", \"verifier\", \"started_at\", \"ends_at\") VALUES \
            ('00000000-0000-7000-8000-000000000011', '00000000-0000-7000-8000-0000000000b1', 'expired key', '2026-01-01T00:00:00Z', '2026-10-18T00:00:00Z'), \
            ('00000000-0000-7000-8000-000000000012', '00000000-0000-7000-8000-0000000000b1', 'live key', '2026-01-01T00:00:00Z', '2026-12-31T00:00:00Z') \
            RETURNING 1")
            .await;

        let ticked = tick(&program, &host, T0).await;
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        assert_eq!(ticked.deleted.sessions, 3, "{:?}", ticked.deleted);
        assert_eq!(ticked.deleted.service_keys, 1, "{:?}", ticked.deleted);
        let sessions = host
            .rows(&host.main, "SELECT \"verifier\" FROM \"jadpo_sessions\" ORDER BY \"verifier\"")
            .await;
        assert_eq!(
            sessions,
            [
                vec![SqlValue::Text("live session".into())],
                vec![SqlValue::Text("live token".into())],
            ]
        );
        let keys = host
            .rows(&host.tallies, "SELECT \"verifier\" FROM \"jadpo_service_keys\"")
            .await;
        assert_eq!(keys, [vec![SqlValue::Text("live key".into())]]);

        // 1,000 rows at a time, ten times a tick: what one tick leaves, the
        // next carries on with.
        host.rows(&host.main, "INSERT INTO \"jadpo_sessions\" (\"id\", \"principal_id\", \"verifier\", \"csrf_verifier\", \"started_at\", \"ends_at\") \
            SELECT gen_random_uuid(), '00000000-0000-7000-8000-0000000000a1', 'old ' || n, NULL, '2026-09-01T00:00:00Z', '2026-10-01T00:00:00Z' FROM generate_series(1, 10001) AS n \
            RETURNING 1")
            .await;
        let first = tick(&program, &host, T0 + HOUR).await;
        assert_eq!(first.deleted.sessions, 10_000);
        let second = tick(&program, &host, T0 + HOUR + MINUTE).await;
        assert_eq!(second.deleted.sessions, 1);
        let left = host
            .count(&host.main, "SELECT count(*) FROM \"jadpo_sessions\"")
            .await;
        assert_eq!(left, 2, "the live session and token");
    });
}

#[test]
fn handled_records_go_only_once_their_delivery_can_no_longer_repeat() {
    let Some((program, _databases, host)) = load("handled") else {
        return;
    };
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        // Three notes, each delivered on the next tick to both
        // subscribers: a handled record in `main` and one in `tallies`.
        for text in ["kept", "held by a failure", "held by a retry"] {
            add_note(&program, &host, text).await;
        }
        host.clock.set(T0 + MINUTE);
        let delivered = tick(&program, &host, T0 + MINUTE).await;
        assert_eq!(delivered.deliveries.len(), 6, "{delivered:?}");
        assert_eq!(host.handled().await, (3, 3));

        // The second note's mark failed for good (an operator can retry
        // it), and the third's tally is waiting to be tried again: each
        // could still repeat, so their event and every handled record of
        // it stay.
        let failed = host.rows(&host.main, "UPDATE \"jadpo_deliveries\" SET \"status\" = 'failed' WHERE \"subscriber\" = 'subscriber mark_note_added' AND \"outbox_id\" = (SELECT \"o\".\"id\" FROM \"jadpo_outbox\" AS \"o\" WHERE \"o\".\"payload\" ->> 'note_id' = (SELECT \"id\"::text FROM \"note\" WHERE \"body\" = 'held by a failure')) RETURNING 1")
            .await;
        let waiting = host.rows(&host.main, "UPDATE \"jadpo_deliveries\" SET \"status\" = 'pending', \"next_attempt_at\" = '2027-01-01T00:00:00Z' WHERE \"subscriber\" = 'subscriber tally_note_added' AND \"outbox_id\" = (SELECT \"o\".\"id\" FROM \"jadpo_outbox\" AS \"o\" WHERE \"o\".\"payload\" ->> 'note_id' = (SELECT \"id\"::text FROM \"note\" WHERE \"body\" = 'held by a retry')) RETURNING 1")
            .await;
        assert_eq!((failed.len(), waiting.len()), (1, 1));

        // A fourth note, 23 hours in: every delivery of it succeeded, but
        // its event is kept a day.
        host.clock.set(T0 + 23 * HOUR);
        add_note(&program, &host, "too new").await;
        let young = tick(&program, &host, T0 + 23 * HOUR + MINUTE).await;
        assert_eq!(young.deliveries.len(), 2, "{young:?}");
        assert_eq!(host.handled().await, (4, 4));

        // A day after the first three: the first's event goes with its
        // delivery records, and with them its handled records in both
        // stores; nothing else's.
        host.clock.set(T0 + 24 * HOUR + 5 * MINUTE);
        let day = tick(&program, &host, T0 + 24 * HOUR + 5 * MINUTE).await;
        assert!(day.problems.is_empty(), "{:?}", day.problems);
        assert_eq!(day.deleted.events, 1, "{:?}", day.deleted);
        assert_eq!(day.deleted.handled, 2, "{:?}", day.deleted);
        assert_eq!(host.handled().await, (3, 3));
        let outbox = "SELECT count(*) FROM \"jadpo_outbox\"";
        assert_eq!(host.count(&host.main, outbox).await, 3);
        // The handled records left are those of deliveries still on
        // record.
        let mut deliveries = host
            .rows(&host.main, "SELECT \"id\"::text FROM \"jadpo_deliveries\"")
            .await;
        deliveries.sort_by_key(|row| format!("{row:?}"));
        let mut handled = host
            .rows(&host.main, "SELECT \"id\"::text FROM \"jadpo_handled_deliveries\"")
            .await;
        handled.extend(
            host.rows(&host.tallies, "SELECT \"id\"::text FROM \"jadpo_handled_deliveries\"")
                .await,
        );
        handled.sort_by_key(|row| format!("{row:?}"));
        assert_eq!(handled, deliveries);

        // Once the operator's failure and the retry are settled, a later
        // tick takes their events and handled records too; the young one
        // waits for its own day.
        host.rows(&host.main, "UPDATE \"jadpo_deliveries\" SET \"status\" = 'succeeded', \"next_attempt_at\" = NULL RETURNING 1")
            .await;
        let settled = tick(&program, &host, T0 + 25 * HOUR + 6 * MINUTE).await;
        assert_eq!(settled.deleted.events, 2, "{:?}", settled.deleted);
        assert_eq!(settled.deleted.handled, 4, "{:?}", settled.deleted);
        assert_eq!(host.handled().await, (1, 1));
        let last = tick(&program, &host, T0 + 47 * HOUR).await;
        assert_eq!((last.deleted.events, last.deleted.handled), (1, 2));
        assert_eq!(host.handled().await, (0, 0));
        assert_eq!(host.count(&host.main, outbox).await, 0);
    });
}

/// The id, as 32 hex digits, of the delivery to `subscriber` of the event
/// adding the note `body`.
async fn delivery_of(host: &Stores, body: &str, subscriber: &str) -> String {
    let rows = host
        .rows(
            &host.main,
            &format!(
                "SELECT replace(\"d\".\"id\"::text, '-', '') FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" WHERE \"d\".\"subscriber\" = 'subscriber {subscriber}' AND \"o\".\"payload\" ->> 'note_id' = (SELECT \"id\"::text FROM \"note\" WHERE \"body\" = '{body}')"
            ),
        )
        .await;
    match rows.first().and_then(|row| row.first()) {
        Some(SqlValue::Text(id)) => id.clone(),
        other => panic!("no delivery of `{body}` to {subscriber}: {other:?}"),
    }
}

/// Puts delivery `id` in `state`, as the runner would leave it: `failed`
/// for good, or `pending` until 2027.
async fn leave(host: &Stores, id: &str, state: &str) {
    let due = if state == "pending" {
        "'2027-01-01T00:00:00Z'"
    } else {
        "NULL"
    };
    let rows = host
        .rows(
            &host.main,
            &format!(
                "UPDATE \"jadpo_deliveries\" SET \"status\" = '{state}', \"next_attempt_at\" = {due} WHERE replace(\"id\"::text, '-', '') = '{id}' RETURNING 1"
            ),
        )
        .await;
    assert_eq!(rows.len(), 1, "{id}");
}

/// What `jadpo deliveries <action>` does to delivery `id` at `now`.
async fn operate(host: &Stores, id: &str, action: Action, now: i64) {
    let holds = Holds {
        deliveries: true,
        runs: false,
        steps: false,
        messages: false,
    };
    let found = find(&host.main, holds, id, now)
        .await
        .expect("the store answers");
    let [item] = found.as_slice() else {
        panic!("one delivery `{id}`: {found:?}");
    };
    host.clock.set(now);
    match act_on_delivery(&host.main, host, item, action, "sam", now).await {
        Ok(Done::Logged { .. }) => {}
        other => panic!("{action:?} on `{id}`: {other:?}"),
    }
}

#[test]
fn what_an_operator_settled_goes_after_thirty_days_and_what_could_run_stays() {
    let Some((program, _databases, host)) = load("settled-by-hand") else {
        return;
    };
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        // Five notes, each delivered on the next tick to both
        // subscribers, with a handled record of each in its store.
        let notes = [
            "dismissed early",
            "cancelled early",
            "dismissed late",
            "still failed",
            "still pending",
        ];
        for text in notes {
            add_note(&program, &host, text).await;
        }
        host.clock.set(T0 + MINUTE);
        let delivered = tick(&program, &host, T0 + MINUTE).await;
        assert_eq!(delivered.deliveries.len(), 10, "{delivered:?}");
        assert_eq!(host.handled().await, (5, 5));

        // One delivery of each note is left that didn't succeed, as a
        // worker given up at its claim leaves one beside a handled record;
        // an operator dismisses one at once and cancels another, retries
        // a third (which fails again), and dismisses one 20 days on.
        let early = delivery_of(&host, "dismissed early", "mark_note_added").await;
        let cancelled = delivery_of(&host, "cancelled early", "tally_note_added").await;
        let late = delivery_of(&host, "dismissed late", "mark_note_added").await;
        let failed = delivery_of(&host, "still failed", "mark_note_added").await;
        let pending = delivery_of(&host, "still pending", "tally_note_added").await;
        for id in [&early, &late, &failed] {
            leave(&host, id, "failed").await;
        }
        for id in [&cancelled, &pending] {
            leave(&host, id, "pending").await;
        }
        operate(&host, &early, Action::Dismiss, T0 + 2 * MINUTE).await;
        operate(&host, &cancelled, Action::Cancel, T0 + 2 * MINUTE).await;
        operate(&host, &failed, Action::Retry, T0 + 2 * MINUTE).await;
        leave(&host, &failed, "failed").await;
        operate(&host, &late, Action::Dismiss, T0 + 20 * DAY).await;
        let log = "SELECT count(*) FROM \"jadpo_operator_actions\"";
        let outbox = "SELECT count(*) FROM \"jadpo_outbox\"";
        let deliveries = "SELECT count(*) FROM \"jadpo_deliveries\"";
        assert_eq!(host.count(&host.main, log).await, 4);

        // 29 days on, every event still has a delivery an operator
        // settled within the window, or one that could run: nothing goes.
        host.clock.set(T0 + 29 * DAY);
        let kept = tick(&program, &host, T0 + 29 * DAY).await;
        assert!(kept.problems.is_empty(), "{:?}", kept.problems);
        assert_eq!(kept.deleted.deliveries, 0, "{:?}", kept.deleted);
        assert_eq!(kept.deleted.events, 0, "{:?}", kept.deleted);
        assert_eq!(kept.deleted.actions, 0, "{:?}", kept.deleted);
        assert_eq!(host.count(&host.main, outbox).await, 5);
        assert_eq!(host.count(&host.main, deliveries).await, 10);

        // 30 days and a minute after the first dismissal and cancellation:
        // those two deliveries go, with their handled records in both
        // stores, and then their events, each with the delivery of it that
        // succeeded; their log rows go with them. The retried delivery's
        // log row stays while the delivery does.
        host.clock.set(T0 + 30 * DAY + 3 * MINUTE);
        let month = tick(&program, &host, T0 + 30 * DAY + 3 * MINUTE).await;
        assert!(month.problems.is_empty(), "{:?}", month.problems);
        assert_eq!(month.deleted.deliveries, 2, "{:?}", month.deleted);
        assert_eq!(month.deleted.events, 2, "{:?}", month.deleted);
        assert_eq!(month.deleted.handled, 4, "{:?}", month.deleted);
        assert_eq!(month.deleted.actions, 2, "{:?}", month.deleted);
        assert_eq!(host.handled().await, (3, 3));
        assert_eq!(host.count(&host.main, outbox).await, 3);
        assert_eq!(host.count(&host.main, deliveries).await, 6);
        let left = host
            .rows(&host.main, "SELECT \"action\", \"was\" FROM \"jadpo_operator_actions\" ORDER BY \"acted_at\"")
            .await;
        assert_eq!(
            left,
            [
                vec![SqlValue::Text("retry".into()), SqlValue::Text("failed".into())],
                vec![SqlValue::Text("dismiss".into()), SqlValue::Text("failed".into())],
            ]
        );

        // 50 days and more on, the late dismissal goes the same way; the
        // failed delivery and the pending one stay, with their events,
        // handled records and log row, however old.
        host.clock.set(T0 + 51 * DAY);
        let later = tick(&program, &host, T0 + 51 * DAY).await;
        assert!(later.problems.is_empty(), "{:?}", later.problems);
        assert_eq!(later.deleted.deliveries, 1, "{:?}", later.deleted);
        assert_eq!(later.deleted.events, 1, "{:?}", later.deleted);
        assert_eq!(later.deleted.handled, 2, "{:?}", later.deleted);
        assert_eq!(later.deleted.actions, 1, "{:?}", later.deleted);
        assert_eq!(host.handled().await, (2, 2));
        assert_eq!(host.count(&host.main, outbox).await, 2);
        let states = host
            .rows(&host.main, "SELECT \"status\" FROM \"jadpo_deliveries\" WHERE \"status\" <> 'succeeded' ORDER BY \"status\"")
            .await;
        assert_eq!(
            states,
            [
                vec![SqlValue::Text("failed".into())],
                vec![SqlValue::Text("pending".into())],
            ]
        );
        assert_eq!(host.count(&host.main, log).await, 1);

        // 1,000 at a time, ten times a tick: what one tick leaves, the
        // next carries on with, and the event goes once its last
        // delivery has.
        host.rows(&host.main, "INSERT INTO \"jadpo_outbox\" (\"id\", \"event\", \"payload\", \"emitted_at\") VALUES ('00000000-0000-7000-8000-0000000000e1', 'event NoteAdded', '{}', '2026-10-01T00:00:00Z') RETURNING 1")
            .await;
        host.rows(&host.main, "INSERT INTO \"jadpo_deliveries\" (\"id\", \"outbox_id\", \"subscriber\", \"status\", \"attempts\") \
            SELECT gen_random_uuid(), '00000000-0000-7000-8000-0000000000e1', 'subscriber ' || n, 'dismissed', 1 FROM generate_series(1, 10001) AS n \
            RETURNING 1")
            .await;
        let first = tick(&program, &host, T0 + 52 * DAY).await;
        assert_eq!(first.deleted.deliveries, 10_000, "{:?}", first.deleted);
        assert_eq!(first.deleted.events, 0, "{:?}", first.deleted);
        let second = tick(&program, &host, T0 + 52 * DAY + MINUTE).await;
        assert_eq!(second.deleted.deliveries, 1, "{:?}", second.deleted);
        assert_eq!(second.deleted.events, 1, "{:?}", second.deleted);
        assert_eq!(host.count(&host.main, outbox).await, 2);
    });
}

#[test]
fn hourly_housekeeping_failure_in_one_store_does_not_suppress_another() {
    let Some((program, _databases, host)) = load("hourly-independent") else {
        return;
    };
    Runtime::new().unwrap().block_on(async {
        host.rows(&host.main,"ALTER TABLE jadpo_idempotency_keys RENAME TO hidden_keys").await;
        let failed=tick(&program,&host,T0).await;
        assert!(!failed.problems.is_empty());
        assert_eq!(host.count(&host.main,"SELECT count(*) FROM jadpo_housekeeping WHERE last_cleaned_at IS NULL AND lease_id IS NULL").await,1);
        assert_eq!(host.count(&host.tallies,"SELECT count(*) FROM jadpo_housekeeping WHERE last_cleaned_at IS NOT NULL AND lease_id IS NULL").await,1);
        host.rows(&host.main,"ALTER TABLE hidden_keys RENAME TO jadpo_idempotency_keys").await;
        let recovered=tick(&program,&host,T0+MINUTE).await;
        assert!(recovered.problems.is_empty(),"{recovered:?}");
        assert_eq!(host.count(&host.main,"SELECT count(*) FROM jadpo_housekeeping WHERE last_cleaned_at IS NOT NULL AND lease_id IS NULL").await,1);
        // The successful store's completion is unchanged on the next tick.
        assert_eq!(host.count(&host.tallies,"SELECT count(*) FROM jadpo_housekeeping WHERE last_cleaned_at='2026-10-19T09:00:00Z'").await,1);
    });
}
