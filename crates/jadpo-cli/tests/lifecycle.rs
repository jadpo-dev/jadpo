//! Lifecycles run against live PostgreSQL (area 5, "Lifecycle"; area 6,
//! "The operations every entity gets"): through `jadpo test`, a record its
//! lifecycle hides is in no query, `get`, update or delete and reads as
//! not found; `delete` runs the lifecycle's `delete` transition, so the
//! row stays; a transition whose `when:` doesn't hold is refused as the
//! conflict its arm names, by `id` and on a loaded record; a parent stays
//! included when hidden while hidden children are left out; and records
//! published in the future stay hidden until then. Then, through the
//! runtime's purge entry point, hidden records go once they have been
//! hidden for `after:`, with what cascades from them, while visible
//! records, records hidden too recently and records a `restricted`
//! reference holds stay, directly or through any chain of cascades, one
//! that comes back to the entity itself among them.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_interp::Program;
use jadpo_runtime::{Clock, Database, Executor, Outcome, PURGE_BATCHES, Row, SqlValue};
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
    ("app.jadpo", "application {}\n"),
    (
        "domains/club/types/name.jadpo",
        "/// A name.\ntype Name = Text {\n    max_length: 80\n}\n",
    ),
    (
        "domains/club/types/stage.jadpo",
        "/// Where a member is.\ntype Stage = Enum {\n    pending\n    active\n    lapsed\n}\n",
    ),
    (
        "domains/club/entities/member.jadpo",
        "use club/types { Name, Stage }

/// Someone in the club.
entity Member {
    id: Uuid
    name: Name
    stage: Stage
    left_at: Instant?
    lapsed_at: Instant?
    changed_at: Instant {
        generated: Generated.on_change
    }

    lifecycle: {
        visible: left_at == none
        initial: {
            stage: Stage.pending
        }
        transitions: {
            delete: {
                when: left_at == none
                set: {
                    left_at: clock.now
                }
            }
            activate: {
                when: stage == Stage.pending
                set: {
                    stage: Stage.active
                }
            }
            lapse: {
                when: stage == Stage.active
                set: {
                    stage: Stage.lapsed
                    lapsed_at: clock.now
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
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
}
",
    ),
    (
        "domains/club/entities/visit.jadpo",
        "use club/entities { Member }\nuse club/types { Name }\n\n/// A member's visit.\nentity Visit {\n    id: Uuid\n    member_id: Member.id {\n        on_delete: OnDelete.cascade\n    }\n    note: Name\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n        update: [Access.public]\n        delete: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/club/entities/badge.jadpo",
        "use club/entities { Member }\n\n/// A badge a member holds, which keeps them from being purged.\nentity Badge {\n    id: Uuid\n    member_id: Member.id {\n        on_delete: OnDelete.restricted\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n        update: [Access.public]\n        delete: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/club/entities/notice.jadpo",
        "use club/types { Name }\n\n/// A notice shown from its time.\nentity Notice {\n    id: Uuid\n    title: Name\n    published_at: Instant?\n\n    lifecycle: {\n        visible: published_at != none and published_at <= clock.now\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n        update: [Access.public]\n        delete: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/club/types/member_view.jadpo",
        "use club/entities { Member }\n\n/// A member, briefly.\ntype MemberView = Output {\n    pick: [Member.id, Member.name]\n}\n",
    ),
    (
        "domains/club/types/visit_view.jadpo",
        "use club/entities { Visit }\n\n/// A visit, briefly.\ntype VisitView = Output {\n    pick: [Visit.id, Visit.note]\n}\n",
    ),
    (
        "domains/club/types/member_page.jadpo",
        "use club/entities { Member }\nuse club/types { VisitView }\n\n/// A member with their visits.\ntype MemberPage = Output {\n    pick: [Member.id, Member.name]\n    visits: List<VisitView>\n}\n",
    ),
    (
        "domains/club/types/visit_page.jadpo",
        "use club/entities { Visit }\nuse club/types { MemberView }\n\n/// A visit with its member.\ntype VisitPage = Output {\n    pick: [Visit.id, Visit.note]\n    member: MemberView\n}\n",
    ),
    (
        "domains/club/types/notice_view.jadpo",
        "use club/entities { Notice }\n\n/// A notice, briefly.\ntype NoticeView = Output {\n    pick: [Notice.id, Notice.title]\n}\n",
    ),
    (
        "domains/club/failures/member_not_found.jadpo",
        "/// No member has that id.\ntype MemberNotFound = Failure {\n    kind: FailureKind.not_found\n}\n",
    ),
    (
        "domains/club/failures/visit_not_found.jadpo",
        "/// No visit has that id.\ntype VisitNotFound = Failure {\n    kind: FailureKind.not_found\n}\n",
    ),
    (
        "domains/club/failures/not_pending.jadpo",
        "/// The member isn't waiting to be activated.\ntype NotPending = Failure {\n    kind: FailureKind.conflict\n}\n",
    ),
    (
        "domains/club/failures/not_active.jadpo",
        "/// The member isn't active.\ntype NotActive = Failure {\n    kind: FailureKind.conflict\n}\n",
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
        "domains/club/actions/activate.jadpo",
        "use club/entities { Member }\nuse club/failures { MemberNotFound, NotPending }\n\n/// Activates a member.\naction activate(member_id: Member.id) fails MemberNotFound, NotPending -> Member {\n    return attempt Member.activate(member_id) {\n        conflict: {\n            Member.activate: NotPending\n        }\n    }\n}\n",
    ),
    (
        "domains/club/actions/lapse.jadpo",
        "use club/entities { Member }\nuse club/failures { NotActive }\n\n/// A member lapses.\naction lapse(member: Member) fails NotActive -> Member {\n    return attempt Member.lapse(member) {\n        conflict: {\n            Member.lapse: NotActive\n        }\n    }\n}\n",
    ),
    (
        "domains/club/actions/rename.jadpo",
        "use club/entities { Member }\nuse club/failures { MemberNotFound }\n\n/// Renames a member, by id.\naction rename(member_id: Member.id, name: Member.name) fails MemberNotFound -> Member {\n    return attempt Member.update(member_id) {\n        set: {\n            name: name\n        }\n    }\n}\n",
    ),
    (
        "domains/club/actions/look_up.jadpo",
        "use club/entities { Member }\nuse club/failures { MemberNotFound }\n\n/// Reads a member.\naction look_up(member_id: Member.id) fails MemberNotFound -> Member {\n    return attempt Member.get(member_id)\n}\n",
    ),
    (
        "domains/club/actions/record_visit.jadpo",
        "use club/entities { Member, Visit }\n\n/// Records a visit.\naction record_visit(member: Member, note: Visit.note) -> Visit {\n    return Visit.create {\n        set: {\n            member_id: member.id\n            note: note\n        }\n    }\n}\n",
    ),
    (
        "domains/club/actions/award.jadpo",
        "use club/entities { Badge, Member }\n\n/// Gives a member a badge.\naction award(member: Member) -> Badge {\n    return Badge.create {\n        set: {\n            member_id: member.id\n        }\n    }\n}\n",
    ),
    (
        "domains/club/actions/post_notice.jadpo",
        "use club/entities { Notice }\n\n/// Posts a notice.\naction post_notice(title: Notice.title, published_at: Notice.published_at) -> Notice {\n    return Notice.create {\n        set: {\n            title: title\n            published_at: published_at\n        }\n    }\n}\n",
    ),
    (
        "domains/club/queries/members_named.jadpo",
        "use club/entities { Member }\nuse club/types { MemberView }\n\n/// Members with a name.\nquery members_named(name: Member.name) -> List<MemberView> {\n    from: Member\n    where: Member.name == name\n    order: [Member.id asc]\n    limit: 10\n}\n",
    ),
    (
        "domains/club/queries/member_with_visits.jadpo",
        "use club/entities { Member, Visit }\nuse club/types { MemberPage }\nuse club/failures { MemberNotFound }\n\n/// A member with their visits.\nquery member_with_visits(member_id: Member.id) fails MemberNotFound -> MemberPage {\n    from: Member\n    where: Member.id == member_id\n    missing: MemberNotFound\n    include: {\n        visits: {\n            via: Visit.member_id\n            order: [Visit.note asc]\n            limit: 10\n        }\n    }\n}\n",
    ),
    (
        "domains/club/queries/visit_with_member.jadpo",
        "use club/entities { Visit }\nuse club/types { VisitPage }\nuse club/failures { VisitNotFound }\n\n/// A visit with its member.\nquery visit_with_member(visit_id: Visit.id) fails VisitNotFound -> VisitPage {\n    from: Visit\n    where: Visit.id == visit_id\n    missing: VisitNotFound\n    include: {\n        member: Visit.member_id\n    }\n}\n",
    ),
    (
        "domains/club/queries/notices.jadpo",
        "use club/entities { Notice }\nuse club/types { NoticeView }\n\n/// The notices showing now.\nquery notices() -> List<NoticeView> {\n    from: Notice\n    order: [Notice.title asc]\n    limit: 10\n}\n",
    ),
];

const TESTS: &str = "use club/entities { Member, Notice, Visit }
use club/types { MemberPage, Stage, VisitPage }
use club/failures { MemberNotFound, NotActive, NotPending }
use club/actions { activate, lapse, leave, look_up, post_notice, record_visit, rename }
use club/queries { member_with_visits, members_named, notices, visit_with_member }

test \"a member who leaves is hidden from every read and write\" {
    var ada: Member = join(\"Ada\")
    assert ada.stage == Stage.pending
    assert ada.left_at == none
    attempt leave(ada.id)
    assert look_up(ada.id) fails MemberNotFound
    assert rename(ada.id, \"Ada B\") fails MemberNotFound
    assert leave(ada.id) fails MemberNotFound
    assert activate(ada.id) fails MemberNotFound
    assert members_named(\"Ada\") == []
    assert member_with_visits(ada.id) fails MemberNotFound
}

test \"transitions move a member through its stages\" {
    var bob: Member = join(\"Bob\")
    var active: Member = attempt activate(bob.id)
    assert active.stage == Stage.active
    assert activate(bob.id) fails NotPending
    var lapsed: Member = attempt lapse(active)
    assert lapsed.stage == Stage.lapsed
    assert lapsed.lapsed_at != none
    assert lapse(lapsed) fails NotActive
    var found: Member = attempt look_up(bob.id)
    assert found.stage == Stage.lapsed
}

test \"a parent stays included when hidden, and children of a visible parent show\" {
    var cy: Member = join(\"Cy\")
    var first: Visit = record_visit(cy, \"first\")
    var page: MemberPage = attempt member_with_visits(cy.id)
    assert list.map(page.visits, v -> v.note) == [\"first\"]
    attempt leave(cy.id)
    assert member_with_visits(cy.id) fails MemberNotFound
    var seen: VisitPage = attempt visit_with_member(first.id)
    assert seen.member.name == \"Cy\"
}

test \"a notice shows from its time\" {
    var earlier: Notice = post_notice(\"Earlier\", \"1999-12-31T00:00:00Z\")
    var later: Notice = post_notice(\"Later\", \"2000-01-02T00:00:00Z\")
    var draft: Notice = post_notice(\"Draft\", none)
    assert later.published_at != none
    assert draft.published_at == none
    assert earlier.published_at != none
    assert list.map(notices(), n -> n.title) == [\"Earlier\"]
}
";

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("lifecycle-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

#[test]
fn lifecycles_hide_records_and_run_transitions() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("semantics");
    write(&root, "domains/club/actions/join.test.jadpo", TESTS);
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("4 passed"), "{}", tested.stdout);
}

// --- purge -----------------------------------------------------------------------

/// Runs `action` with `args`, as one invocation at the test clock.
async fn call(program: &Program, host: &Harness, action: &str, args: Vec<Value>) -> Value {
    let id = program.plan().find(action).expect("the action");
    // Each invocation its own seed, so each makes new identities.
    let mut ctx = host.ctx(program);
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

async fn rows(store: &Postgres, sql: &str) -> Vec<Row> {
    let mut transaction = store.begin().await.expect("begin");
    let rows = transaction.query(sql, &[]).await.expect("query");
    transaction.commit().await.expect("commit");
    rows
}

async fn names(store: &Postgres, table: &str) -> Vec<String> {
    let sql = match table {
        "member" => "SELECT \"name\" FROM \"member\" ORDER BY \"name\"",
        _ => "SELECT \"note\" FROM \"visit\" ORDER BY \"note\"",
    };
    rows(store, sql)
        .await
        .into_iter()
        .map(|row| match &row[0] {
            SqlValue::Text(text) => text.clone(),
            other => panic!("not text: {other:?}"),
        })
        .collect()
}

async fn folders(store: &Postgres) -> Vec<String> {
    rows(store, "SELECT \"name\" FROM \"folder\" ORDER BY \"name\"")
        .await
        .into_iter()
        .map(|row| match &row[0] {
            SqlValue::Text(text) => text.clone(),
            other => panic!("not text: {other:?}"),
        })
        .collect()
}

const DAY: i64 = 86_400_000;

#[test]
fn purge_deletes_records_hidden_long_enough_and_keeps_the_rest() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("purge");
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let plan = program
        .plan()
        .purge("Member")
        .expect("Member's purge")
        .clone();
    assert!(program.plan().purge("Notice").is_none(), "no purge: none");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url);
    let check = postgres(&url).expect("store");
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        // Ada stays visible; Bob and Cy leave, and Cy holds a badge; Dee
        // leaves with two visits, which go with her.
        let ada = call(&program, &host, "join", vec![Value::text("Ada")]).await;
        let bob = call(&program, &host, "join", vec![Value::text("Bob")]).await;
        let cy = call(&program, &host, "join", vec![Value::text("Cy")]).await;
        let dee = call(&program, &host, "join", vec![Value::text("Dee")]).await;
        call(
            &program,
            &host,
            "record_visit",
            vec![ada.clone(), Value::text("ada's")],
        )
        .await;
        call(
            &program,
            &host,
            "record_visit",
            vec![dee.clone(), Value::text("dee's 1")],
        )
        .await;
        call(
            &program,
            &host,
            "record_visit",
            vec![dee.clone(), Value::text("dee's 2")],
        )
        .await;
        call(&program, &host, "award", vec![cy.clone()]).await;
        for member in [&bob, &cy, &dee] {
            call(&program, &host, "leave", vec![id_of(member)]).await;
        }
        assert_eq!(
            names(&check, "member").await,
            ["Ada", "Bob", "Cy", "Dee"],
            "leaving hides a member, and keeps the row"
        );

        // Hidden for 29 days: not yet.
        let early = jadpo_runtime::purge(&host, &plan, Clock::TEST.now() + 29 * DAY, PURGE_BATCHES)
            .await
            .expect("purge");
        assert_eq!(early.removed(), 0, "{early:?}");
        let at_cutoff =
            jadpo_runtime::purge(&host, &plan, Clock::TEST.now() + 30 * DAY, PURGE_BATCHES)
                .await
                .expect("purge at the retention boundary");
        assert_eq!(
            at_cutoff.removed(),
            0,
            "only older than the cutoff: {at_cutoff:?}"
        );
        assert_eq!(names(&check, "member").await, ["Ada", "Bob", "Cy", "Dee"]);

        // Hidden for 31 days: Bob and Dee go, with Dee's visits; Cy's badge
        // holds her; Ada was never hidden.
        let purged =
            jadpo_runtime::purge(&host, &plan, Clock::TEST.now() + 31 * DAY, PURGE_BATCHES)
                .await
                .expect("purge");
        assert_eq!(purged.entity, "Member");
        assert_eq!(purged.batches, vec![2], "one batch, short of a whole one");
        assert_eq!(names(&check, "member").await, ["Ada", "Cy"]);
        assert_eq!(names(&check, "visit").await, ["ada's"], "cascaded with Dee");

        // Run again: nothing left to purge.
        let again = jadpo_runtime::purge(&host, &plan, Clock::TEST.now() + 31 * DAY, PURGE_BATCHES)
            .await
            .expect("purge");
        assert_eq!(again.removed(), 0);

        // Once Cy's badge goes, so does she.
        let mut transaction = check.begin().await.expect("begin");
        transaction
            .batch("DELETE FROM \"badge\"")
            .await
            .expect("delete");
        transaction.commit().await.expect("commit");
        let last = jadpo_runtime::purge(&host, &plan, Clock::TEST.now() + 31 * DAY, PURGE_BATCHES)
            .await
            .expect("purge");
        assert_eq!(last.removed(), 1);
        assert_eq!(names(&check, "member").await, ["Ada"]);
    });
    drop(host);
    drop(check);
    databases.release(copy, false);
}

#[test]
fn purge_waits_as_long_as_a_setting_says() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("purge-setting");
    // The period is a setting (area 10, "Retention periods"), not a
    // duration written in place.
    let member = root.join("domains/club/entities/member.jadpo");
    let text = fs::read_to_string(&member).expect("the entity");
    assert!(text.contains("after: 30d"));
    write(
        &root,
        "domains/club/entities/member.jadpo",
        &text.replacen("after: 30d", "after: config.retention", 1),
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    retention: Duration {\n        default: 30d\n    }\n}\n",
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let plan = program
        .plan()
        .purge("Member")
        .expect("Member's purge")
        .clone();
    assert_eq!(plan.after_setting, Some(0));
    let values = program
        .prepared()
        .config(
            &|_: &str| None,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .expect("the default holds");
    assert_eq!(
        program.prepared().purge_after(&values, &plan),
        Some(30 * DAY)
    );
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url);
    let check = postgres(&url).expect("store");
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let bob = call(&program, &host, "join", vec![Value::text("Bob")]).await;
        call(&program, &host, "leave", vec![id_of(&bob)]).await;
        // Without the period it is given, a purge deletes nothing.
        assert!(
            jadpo_runtime::purge(&host, &plan, Clock::TEST.now() + 31 * DAY, PURGE_BATCHES)
                .await
                .is_err()
        );
        let after = program
            .prepared()
            .purge_after(&values, &plan)
            .expect("a period");
        let early = jadpo_runtime::purge_after(
            &host,
            &plan,
            Clock::TEST.now() + 29 * DAY,
            PURGE_BATCHES,
            after,
        )
        .await
        .expect("purge");
        assert_eq!(early.removed(), 0, "{early:?}");
        let at_cutoff = jadpo_runtime::purge_after(
            &host,
            &plan,
            Clock::TEST.now() + 30 * DAY,
            PURGE_BATCHES,
            after,
        )
        .await
        .expect("configured retention at the boundary");
        assert_eq!(
            at_cutoff.removed(),
            0,
            "only older than the configured cutoff"
        );
        assert_eq!(names(&check, "member").await, ["Bob"]);
        let purged = jadpo_runtime::purge_after(
            &host,
            &plan,
            Clock::TEST.now() + 31 * DAY,
            PURGE_BATCHES,
            after,
        )
        .await
        .expect("purge");
        assert_eq!(purged.removed(), 1, "{purged:?}");
        assert!(names(&check, "member").await.is_empty());
        // A period that is nothing would delete at once: it deletes nothing.
        assert!(
            jadpo_runtime::purge_after(&host, &plan, Clock::TEST.now(), PURGE_BATCHES, 0)
                .await
                .is_err()
        );
    });
    drop(host);
    drop(check);
    databases.release(copy, false);
}

#[test]
fn purge_works_in_batches() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("batches");
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let plan = program
        .plan()
        .purge("Member")
        .expect("Member's purge")
        .clone();
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url);
    let check = postgres(&url).expect("store");
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        // 1201 members who left a year ago, written straight to the table.
        let mut transaction = check.begin().await.expect("begin");
        transaction
            .batch(
                "INSERT INTO \"member\" (\"id\", \"name\", \"stage\", \"left_at\", \"lapsed_at\", \"changed_at\") \
                 SELECT gen_random_uuid(), 'gone ' || n, 'pending', TIMESTAMPTZ '1999-01-01', NULL, now() \
                 FROM generate_series(1, 1201) AS n",
            )
            .await
            .expect("insert");
        transaction.commit().await.expect("commit");
        // A run stops at its budget; the next carries on.
        let first = jadpo_runtime::purge(&host, &plan, Clock::TEST.now(), 2)
            .await
            .expect("purge");
        assert_eq!(first.batches, vec![500, 500]);
        assert!(!first.finished, "stopped at its budget");
        assert_eq!(names(&check, "member").await.len(), 201);
        let purged = jadpo_runtime::purge(&host, &plan, Clock::TEST.now(), PURGE_BATCHES)
            .await
            .expect("purge");
        assert_eq!(purged.batches, vec![201]);
        assert!(purged.finished);
        assert!(names(&check, "member").await.is_empty());
    });
    drop(host);
    drop(check);
    databases.release(copy, false);
}

/// Folders inside folders: deleting one cascades to those inside it, so a
/// document anywhere below holds the top one (a reference that cascades
/// back to its own entity).
const FOLDERS: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/files/types/label.jadpo",
        "/// A label.\ntype Label = Text {\n    max_length: 80\n}\n",
    ),
    (
        "domains/files/entities/folder.jadpo",
        "use files/types { Label }

/// A folder, inside another or at the top.
entity Folder {
    id: Uuid
    parent_id: Folder.id? {
        on_delete: OnDelete.cascade
    }
    name: Label
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                when: deleted_at == none
                set: {
                    deleted_at: clock.now
                }
            }
        }
        purge: {
            after: 30d
            since: deleted_at
        }
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
}
",
    ),
    (
        "domains/files/entities/doc.jadpo",
        "use files/entities { Folder }\nuse files/types { Label }\n\n/// A document, which keeps its folder from going.\nentity Doc {\n    id: Uuid\n    folder_id: Folder.id {\n        on_delete: OnDelete.restricted\n    }\n    name: Label\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n        update: [Access.public]\n        delete: [Access.public]\n    }\n}\n",
    ),
];

#[test]
fn purge_follows_a_cascade_back_to_its_own_entity() {
    if !jadpo_test::live() {
        return;
    }
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lifecycle-folders");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FOLDERS {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let plan = program
        .plan()
        .purge("Folder")
        .expect("Folder's purge")
        .clone();
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url);
    let check = postgres(&url).expect("store");
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        // Deleted a year ago: "a" holds "a/b", whose document holds both;
        // "c" holds "c/d" and "c/d/e", which hold nothing. "f" is a
        // folder inside itself, which a purge still finishes with.
        let mut transaction = check.begin().await.expect("begin");
        transaction
            .batch(
                "INSERT INTO \"folder\" (\"id\", \"parent_id\", \"name\", \"deleted_at\") VALUES \
                 ('00000000-0000-7000-8000-00000000000a', NULL, 'a', TIMESTAMPTZ '1999-01-01'), \
                 ('00000000-0000-7000-8000-00000000000b', '00000000-0000-7000-8000-00000000000a', 'a/b', NULL), \
                 ('00000000-0000-7000-8000-00000000000c', NULL, 'c', TIMESTAMPTZ '1999-01-01'), \
                 ('00000000-0000-7000-8000-00000000000d', '00000000-0000-7000-8000-00000000000c', 'c/d', NULL), \
                 ('00000000-0000-7000-8000-00000000000e', '00000000-0000-7000-8000-00000000000d', 'c/d/e', NULL), \
                 ('00000000-0000-7000-8000-00000000000f', '00000000-0000-7000-8000-00000000000f', 'f', TIMESTAMPTZ '1999-01-01'); \
                 INSERT INTO \"doc\" (\"id\", \"folder_id\", \"name\") VALUES \
                 ('00000000-0000-7000-8000-0000000000d0', '00000000-0000-7000-8000-00000000000b', 'kept');",
            )
            .await
            .expect("insert");
        transaction.commit().await.expect("commit");

        let purged = jadpo_runtime::purge(&host, &plan, Clock::TEST.now(), PURGE_BATCHES)
            .await
            .expect("a held folder never stops the batch");
        assert_eq!(purged.removed(), 2, "{purged:?}");
        assert_eq!(folders(&check).await, ["a", "a/b"]);

        // Once the document goes, so do "a" and "a/b".
        let mut transaction = check.begin().await.expect("begin");
        transaction
            .batch("DELETE FROM \"doc\"")
            .await
            .expect("delete");
        transaction.commit().await.expect("commit");
        let last = jadpo_runtime::purge(&host, &plan, Clock::TEST.now(), PURGE_BATCHES)
            .await
            .expect("purge");
        assert_eq!(last.removed(), 1);
        assert!(folders(&check).await.is_empty());
    });
    drop(host);
    drop(check);
    databases.release(copy, false);
}
