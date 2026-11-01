//! Erasing a person against live PostgreSQL (area 15, "Erasure"; chunk
//! P3): `Member.erase(member_id)` deletes the member and, in the same
//! statement, each record of theirs as the reference linking it says:
//!
//! - their loans, returned (hidden by the lifecycle) or not, with the
//!   renewals that cascade from them, and each loan's notes, whose
//!   restricted reference says `Erasure.deleted`, are deleted outright;
//! - their reviews, whose reference is cleared, are kept without the
//!   reference or the name they were signed with, and count as changed;
//! - their fines, kept with `Erasure.retained`, stay as they are with
//!   their lines, only the reference cleared;
//! - their sessions go with them, and another member's records, sessions
//!   and record stay.
//!
//! Policy lets a member erase only themselves: another member's erasure
//! of them is their not-found failure and changes no row. A member who
//! has left, whom the lifecycle hides, is erased too, and erasing a
//! member who isn't there is their not-found failure.
//!
//! Its tests run interpreted through `jadpo test` here, and compiled with
//! the interpreter's records in the gate's compiled step
//! (`crates/jadpo-cli/tests/compiled.rs`, "an erasure"). The rows are read
//! here after a run through the interpreter.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::{Caller, Executor, Outcome, SqlValue};
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

const OPEN: &str = "    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
";

/// A library's members and what they leave behind, with an action for
/// each and the tests erasure must pass.
pub(crate) fn files() -> Vec<(String, String)> {
    let action = |name: &str, uses: &str, signature: &str, body: &str| {
        (
            format!("domains/lending/actions/{name}.jadpo"),
            format!("{uses}\n\n/// {name}.\naction {name}{signature} {{\n{body}}}\n"),
        )
    };
    let mut files: Vec<(String, String)> = vec![
        (
            "app.jadpo".to_string(),
            "use lending/entities { Member }\n\napplication {}\n\nprincipal {\n    user: {\n        subject: Member.handle\n    }\n}\n\nauthentication {\n    api_token: {}\n}\n".to_string(),
        ),
        (
            "config.jadpo".to_string(),
            "config {\n    fine_retention: Duration {\n        default: 2192d\n    }\n}\n".to_string(),
        ),
        (
            "domains/lending/types/name.jadpo".to_string(),
            "/// A name, a title or a line of text.\ntype Name = Text {\n    max_length: 80\n}\n".to_string(),
        ),
        (
            "domains/lending/failures/handle_taken.jadpo".to_string(),
            "/// Someone has that handle already.\ntype HandleTaken = Failure {\n    kind: FailureKind.conflict\n}\n".to_string(),
        ),
        (
            "domains/lending/entities/member.jadpo".to_string(),
            "use lending/types { Name }

/// Someone with a library card, who signs in, and may erase only
/// themselves.
entity Member {
    id: Uuid
    handle: Name {
        unique: true
        personal: PersonalData.identifying
    }
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
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [id]
        delete: [id]
    }

    person: true
}
"
            .to_string(),
        ),
        (
            "domains/lending/entities/loan.jadpo".to_string(),
            format!(
                "use lending/entities {{ Member }}
use lending/types {{ Name }}

/// A book a member borrowed, hidden once it is returned, and renewed as a
/// loan of its own.
entity Loan {{
    id: Uuid
    member_id: Member.id {{
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }}
    title: Name
    renewal_of: Loan.id? {{
        on_delete: OnDelete.cascade
    }}
    returned_at: Instant?

    lifecycle: {{
        visible: returned_at == none
        transitions: {{
            delete: {{
                when: returned_at == none
                set: {{
                    returned_at: clock.now
                }}
            }}
        }}
    }}

{OPEN}}}
"
            ),
        ),
        (
            "domains/lending/entities/loan_note.jadpo".to_string(),
            format!(
                "use lending/entities {{ Loan }}
use lending/types {{ Name }}

/// A librarian's note on a loan.
entity LoanNote {{
    id: Uuid
    loan_id: Loan.id {{
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }}
    body: Name

{OPEN}}}
"
            ),
        ),
        (
            "domains/lending/entities/review.jadpo".to_string(),
            format!(
                "use lending/entities {{ Member }}
use lending/types {{ Name }}

/// A review a member wrote, kept without them.
entity Review {{
    id: Uuid
    member_id: Member.id? {{
        on_delete: OnDelete.cleared
    }}
    body: Name
    signed_as: Name? {{
        personal: PersonalData.identifying
    }}
    changed_at: Instant {{
        generated: Generated.on_change
    }}

{OPEN}}}
"
            ),
        ),
        (
            "domains/lending/entities/fine.jadpo".to_string(),
            format!(
                "use lending/entities {{ Member }}
use lending/types {{ Name }}

/// A fine, kept for the accounts as it is.
entity Fine {{
    id: Uuid
    member_id: Member.id? {{
        on_delete: OnDelete.restricted
        erase: Erasure.retained
    }}
    amount: Name
    issued_at: Instant {{
        generated: Generated.on_create
    }}

    retention: {{
        after: config.fine_retention
        since: issued_at
    }}

{OPEN}}}
"
            ),
        ),
        (
            "domains/lending/entities/fine_line.jadpo".to_string(),
            format!(
                "use lending/entities {{ Fine }}
use lending/types {{ Name }}

/// A line of a fine, which goes with it.
entity FineLine {{
    id: Uuid
    fine_id: Fine.id {{
        on_delete: OnDelete.cascade
    }}
    note: Name

{OPEN}}}
"
            ),
        ),
    ];
    files.extend([
        action(
            "join",
            "use lending/entities { Member }\nuse lending/failures { HandleTaken }",
            "(handle: Member.handle) fails HandleTaken -> Member",
            "    return attempt Member.create {\n        set: {\n            handle: handle\n        }\n        conflict: {\n            Member.handle: HandleTaken\n        }\n    }\n",
        ),
        action(
            "borrow",
            "use lending/entities { Loan, Member }",
            "(member: Member, title: Loan.title) -> Loan",
            "    return Loan.create {\n        set: {\n            member_id: member.id\n            title: title\n            renewal_of: none\n        }\n    }\n",
        ),
        action(
            "renew",
            "use lending/entities { Loan }",
            "(loan: Loan) -> Loan",
            "    return Loan.create {\n        set: {\n            member_id: loan.member_id\n            title: loan.title\n            renewal_of: loan.id\n        }\n    }\n",
        ),
        action(
            "give_back",
            "use lending/entities { Loan, LoanNotFound }",
            "(loan_id: Loan.id) fails LoanNotFound -> Unit",
            "    attempt Loan.delete(loan_id)\n",
        ),
        action(
            "note",
            "use lending/entities { Loan, LoanNote }",
            "(loan: Loan, body: LoanNote.body) -> LoanNote",
            "    return LoanNote.create {\n        set: {\n            loan_id: loan.id\n            body: body\n        }\n    }\n",
        ),
        action(
            "write_review",
            "use lending/entities { Member, Review }",
            "(member: Member, body: Review.body, signed_as: Review.signed_as) -> Review",
            "    return Review.create {\n        set: {\n            member_id: member.id\n            body: body\n            signed_as: signed_as\n        }\n    }\n",
        ),
        action(
            "charge",
            "use lending/entities { Fine, Member }",
            "(member: Member, amount: Fine.amount) -> Fine",
            "    return Fine.create {\n        set: {\n            member_id: member.id\n            amount: amount\n        }\n    }\n",
        ),
        action(
            "add_line",
            "use lending/entities { Fine, FineLine }",
            "(fine: Fine, note: FineLine.note) -> FineLine",
            "    return FineLine.create {\n        set: {\n            fine_id: fine.id\n            note: note\n        }\n    }\n",
        ),
        action(
            "leave",
            "use lending/entities { Member, MemberNotFound }",
            "(me: Principal.user) fails MemberNotFound -> Unit",
            "    attempt Member.delete(me.id)\n",
        ),
        action(
            "close_account",
            "use lending/entities { Member, MemberNotFound }",
            "(me: Principal.user) fails MemberNotFound -> Unit",
            "    attempt Member.erase(me.id)\n",
        ),
        action(
            "erase_member",
            "use lending/entities { Member, MemberNotFound }",
            "(member_id: Member.id, me: Principal.user) fails MemberNotFound -> Unit",
            "    attempt Member.erase(member_id)\n",
        ),
        action(
            "forget",
            "use lending/entities { Member, MemberNotFound }",
            "(member_id: Member.id) fails MemberNotFound -> Unit",
            "    attempt Member.erase(member_id)\n",
        ),
    ]);
    files.push((
        "domains/lending/actions/close_account.test.jadpo".to_string(),
        TESTS.to_string(),
    ));
    files
}

const TESTS: &str = r#"use lending/entities {
    Fine,
    FineLine,
    Loan,
    LoanNotFound,
    LoanNote,
    LoanNoteNotFound,
    Member,
    MemberNotFound,
    Review
}
use lending/actions {
    add_line,
    borrow,
    charge,
    erase_member,
    give_back,
    join,
    leave,
    note,
    renew,
    write_review
}

test "erasing a member deletes, anonymises or keeps each record as its reference says" {
    var ada: Member = attempt join("ada")
    var bob: Member = attempt join("bob")
    var loan: Loan = borrow(ada, "Middlemarch")
    var renewed: Loan = renew(loan)
    var noted: LoanNote = note(loan, "Coffee stain")
    var bobs: Loan = borrow(bob, "Persuasion")
    var said: Review = write_review(ada, "A long read", "Ada L.")
    var owed: Fine = charge(ada, "GBP 2.50")
    var line: FineLine = add_line(owed, "Late by a week")
    attempt close_account(Principal.user(ada))
    assert Member.get(ada.id) fails MemberNotFound
    assert Loan.get(loan.id) fails LoanNotFound
    assert Loan.get(renewed.id) fails LoanNotFound
    assert LoanNote.get(noted.id) fails LoanNoteNotFound
    var kept: Review = attempt Review.get(said.id)
    assert kept.member_id == none
    assert kept.signed_as == none
    assert kept.body == "A long read"
    var still: Fine = attempt Fine.get(owed.id)
    assert still.member_id == none
    assert still.amount == "GBP 2.50"
    var kept_line: FineLine = attempt FineLine.get(line.id)
    assert kept_line.note == "Late by a week"
    var other: Loan = attempt Loan.get(bobs.id)
    assert other.title == "Persuasion"
    var stays: Member = attempt Member.get(bob.id)
    assert stays.handle == "bob"
}

test "a member erases only themselves" {
    var ada: Member = attempt join("ada")
    var bob: Member = attempt join("bob")
    var loan: Loan = borrow(ada, "Middlemarch")
    assert erase_member(ada.id, Principal.user(bob)) fails MemberNotFound
    var still: Loan = attempt Loan.get(loan.id)
    assert still.title == "Middlemarch"
    attempt erase_member(ada.id, Principal.user(ada))
    assert Loan.get(loan.id) fails LoanNotFound
}

test "a member who has left is erased with their returned loans" {
    var ada: Member = attempt join("ada")
    var loan: Loan = borrow(ada, "Emma")
    attempt give_back(loan.id)
    attempt leave(Principal.user(ada))
    assert Member.get(ada.id) fails MemberNotFound
    attempt close_account(Principal.user(ada))
    assert close_account(Principal.user(ada)) fails MemberNotFound
}
"#;

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("erasure-p3-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files() {
        write(&root, &path, &text);
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
}

#[test]
fn erasure_passes_its_tests() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("tests");
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("3 passed"), "{}", tested.stdout);
}

/// 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
const HOUR: i64 = 3_600_000;

/// Runs `action` with `args` at the harness's clock, with no caller.
async fn call(program: &Program, host: &Harness, action: &str, args: Vec<Value>) -> Value {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program);
    match program.run(&mut ctx, id, args).await {
        Outcome::Value(value) => value,
        other => panic!("{action}: expected a value, got {other:?}"),
    }
}

/// The outcome of `action` with `args` as the member `caller`, signed
/// in, whatever it is.
async fn outcome_as(
    program: &Program,
    host: &Harness,
    action: &str,
    args: Vec<Value>,
    caller: &Value,
) -> Outcome {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program);
    let member = match caller {
        Value::Record(record) => record,
        other => panic!("not a record: {other:?}"),
    };
    let signed_in = jadpo_runtime::http::identity::principal(
        jadpo_std::http::PrincipalKind::User,
        member.get("id").cloned().expect("an id"),
        member.get("handle").cloned().expect("a handle"),
    );
    ctx.set_caller(Some(Caller {
        value: signed_in,
        key: "user".to_string(),
        session: None,
        sign_in: None,
        multi_factor: false,
    }));
    program.run(&mut ctx, id, args).await
}

/// Whether `outcome` is the member's not-found failure.
fn not_found(outcome: Outcome) {
    match outcome {
        Outcome::Failed(failure) => assert_eq!(failure.code, "member_not_found"),
        other => panic!("expected the not-found failure, got {other:?}"),
    }
}

fn id_of(value: &Value) -> Value {
    match value {
        Value::Record(record) => record.get("id").cloned().expect("an id"),
        other => panic!("not a record: {other:?}"),
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

/// The rows, as SQL shows them: every record erasure reaches, what it
/// keeps of each, and the sessions, in the database after an erasure run
/// through the interpreter.
#[test]
fn erasure_leaves_the_rows_its_references_say() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("rows");
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
        let mut members = Vec::new();
        for handle in ["ada", "bob"] {
            let member = call(&program, &host, "join", vec![Value::text(handle)]).await;
            let loan = call(
                &program,
                &host,
                "borrow",
                vec![member.clone(), Value::text(&format!("{handle}'s first"))],
            )
            .await;
            call(&program, &host, "renew", vec![loan.clone()]).await;
            call(
                &program,
                &host,
                "note",
                vec![loan.clone(), Value::text(&format!("{handle}'s note"))],
            )
            .await;
            // A loan returned, which the lifecycle hides.
            let returned = call(
                &program,
                &host,
                "borrow",
                vec![member.clone(), Value::text(&format!("{handle}'s returned"))],
            )
            .await;
            call(&program, &host, "give_back", vec![id_of(&returned)]).await;
            call(
                &program,
                &host,
                "write_review",
                vec![
                    member.clone(),
                    Value::text(&format!("{handle}'s review")),
                    Value::text(&format!("{handle} signed")),
                ],
            )
            .await;
            let fine = call(
                &program,
                &host,
                "charge",
                vec![member.clone(), Value::text(&format!("{handle}'s fine"))],
            )
            .await;
            call(
                &program,
                &host,
                "add_line",
                vec![fine, Value::text(&format!("{handle}'s line"))],
            )
            .await;
            members.push(member);
        }
        // Each member holds an API token's session.
        host.rows(
            "INSERT INTO \"jadpo_sessions\" (\"id\", \"principal_id\", \"verifier\", \"started_at\", \"ends_at\") SELECT gen_random_uuid(), \"id\", 'verifier of ' || \"handle\", TIMESTAMPTZ '2026-10-19 09:00:00+00', TIMESTAMPTZ '2030-01-01 00:00:00+00' FROM \"member\"",
        )
        .await;

        let counts = || async {
            texts(
                &host,
                "SELECT (SELECT count(*) FROM \"member\")::text || ' ' || (SELECT count(*) FROM \"loan\")::text || ' ' || (SELECT count(*) FROM \"loan_note\")::text || ' ' || (SELECT count(*) FROM \"review\" WHERE \"member_id\" IS NULL)::text || ' ' || (SELECT count(*) FROM \"jadpo_sessions\")::text",
            )
            .await
        };
        let before = counts().await;
        assert_eq!(before, ["2 6 2 0 2"]);

        // Bob may not erase Ada: her not-found failure, and nothing
        // changes.
        host.set_now(T0 + HOUR);
        not_found(
            outcome_as(
                &program,
                &host,
                "forget",
                vec![id_of(&members[0])],
                &members[1],
            )
            .await,
        );
        assert_eq!(counts().await, before);

        // An hour on, Ada erases herself.
        match outcome_as(
            &program,
            &host,
            "forget",
            vec![id_of(&members[0])],
            &members[0],
        )
        .await
        {
            Outcome::Value(_) => {}
            other => panic!("expected the erasure, got {other:?}"),
        }

        assert_eq!(
            texts(&host, "SELECT \"handle\" FROM \"member\" ORDER BY 1").await,
            ["bob"]
        );
        assert_eq!(
            texts(&host, "SELECT \"title\" FROM \"loan\" ORDER BY 1").await,
            ["bob's first", "bob's first", "bob's returned"],
            "Ada's loans go, returned and renewed ones too"
        );
        assert_eq!(
            texts(&host, "SELECT \"body\" FROM \"loan_note\" ORDER BY 1").await,
            ["bob's note"]
        );
        assert_eq!(
            texts(
                &host,
                "SELECT \"body\" || ' ' || coalesce(\"signed_as\", 'none') || ' ' || (\"member_id\" IS NULL)::text || ' ' || to_char(\"changed_at\" AT TIME ZONE 'UTC', 'HH24:MI') FROM \"review\" ORDER BY 1"
            )
            .await,
            [
                "ada's review none true 10:00",
                "bob's review bob signed false 09:00"
            ],
            "Ada's review stays without her or her name, and counts as changed"
        );
        assert_eq!(
            texts(
                &host,
                "SELECT \"amount\" || ' ' || (\"member_id\" IS NULL)::text FROM \"fine\" ORDER BY 1"
            )
            .await,
            ["ada's fine true", "bob's fine false"],
            "Ada's fine stays as it is, without her"
        );
        assert_eq!(
            texts(&host, "SELECT \"note\" FROM \"fine_line\" ORDER BY 1").await,
            ["ada's line", "bob's line"]
        );
        assert_eq!(
            texts(
                &host,
                "SELECT \"verifier\" FROM \"jadpo_sessions\" ORDER BY 1"
            )
            .await,
            ["verifier of bob"],
            "Ada's session goes with her"
        );

        // Erasing her again finds no one, and changes nothing.
        not_found(
            outcome_as(
                &program,
                &host,
                "forget",
                vec![id_of(&members[0])],
                &members[0],
            )
            .await,
        );
        assert_eq!(
            texts(&host, "SELECT \"handle\" FROM \"member\" ORDER BY 1").await,
            ["bob"]
        );
    });
}
